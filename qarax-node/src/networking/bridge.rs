use anyhow::{Context, Result};
use tracing::{debug, info};

/// Create a Linux bridge device and bring it up.
pub async fn create_bridge(name: &str) -> Result<()> {
    info!("Creating bridge: {}", name);

    run_cmd("ip", &["link", "add", name, "type", "bridge"])
        .await
        .with_context(|| format!("Failed to create bridge {}", name))?;

    run_cmd("ip", &["link", "set", name, "up"])
        .await
        .with_context(|| format!("Failed to bring up bridge {}", name))?;

    Ok(())
}

/// Assign an IP address to the bridge (gateway for the subnet).
pub async fn set_bridge_ip(name: &str, gateway_cidr: &str) -> Result<()> {
    info!("Setting bridge {} IP to {}", name, gateway_cidr);

    run_cmd("ip", &["addr", "add", gateway_cidr, "dev", name])
        .await
        .with_context(|| format!("Failed to set IP {} on bridge {}", gateway_cidr, name))?;

    Ok(())
}

/// Delete a bridge device.
pub async fn delete_bridge(name: &str) -> Result<()> {
    info!("Deleting bridge: {}", name);

    run_cmd("ip", &["link", "del", name])
        .await
        .with_context(|| format!("Failed to delete bridge {}", name))?;

    Ok(())
}

/// Attach a TAP device to a bridge.
pub async fn attach_to_bridge(tap_name: &str, bridge_name: &str) -> Result<()> {
    debug!("Attaching TAP {} to bridge {}", tap_name, bridge_name);

    run_cmd("ip", &["link", "set", tap_name, "master", bridge_name])
        .await
        .with_context(|| format!("Failed to attach {} to bridge {}", tap_name, bridge_name))?;

    Ok(())
}

/// Bridge an existing NIC: create a bridge, move the NIC's IP to the bridge,
/// and add the NIC as a member. VMs on this bridge share the NIC's L2 network.
///
/// The bridge is given the same MAC as the parent NIC so existing ARP entries
/// (e.g. on the host) remain valid across the transition.  If systemd-networkd
/// is running, we also write runtime configs so networkd stays in sync and
/// does not fight our manual changes.
pub async fn bridge_interface(bridge_name: &str, parent_iface: &str) -> Result<()> {
    info!(
        "Bridging interface {} onto bridge {}",
        parent_iface, bridge_name
    );

    // ── Gather state from parent NIC ─────────────────────────────────────
    let addr_info = run_cmd_output("ip", &["-j", "addr", "show", parent_iface])
        .await
        .with_context(|| format!("Failed to get address info for {}", parent_iface))?;

    let addrs: serde_json::Value =
        serde_json::from_str(&addr_info).with_context(|| "Failed to parse ip addr JSON")?;

    let (ip_cidr, _prefix_len) = extract_ipv4_addr(&addrs)
        .with_context(|| format!("No IPv4 address found on {}", parent_iface))?;

    let parent_mac =
        extract_mac(&addrs).with_context(|| format!("No MAC address found on {}", parent_iface))?;

    let route_info = run_cmd_output("ip", &["-j", "route", "show", "default"])
        .await
        .unwrap_or_default();
    let default_gw = parse_default_gateway(&route_info, parent_iface);

    // ── Write networkd overrides (if applicable) BEFORE migrating ────────
    // The configs describe the target state.  After we manually migrate the
    // IP below, networkctl reload sees "everything matches" and is a no-op.
    let use_networkd = is_networkd_running().await;
    if use_networkd {
        info!("systemd-networkd detected — writing bridge networkd configs");
        write_bridge_networkd_configs(bridge_name, parent_iface, &ip_cidr, default_gw.as_deref())
            .await?;
    }

    // ── Create bridge with parent's MAC ──────────────────────────────────
    run_cmd("ip", &["link", "add", bridge_name, "type", "bridge"])
        .await
        .with_context(|| format!("Failed to create bridge {}", bridge_name))?;

    // Use the parent NIC's MAC on the bridge so the host's ARP cache for
    // our IP keeps working throughout the transition.
    run_cmd("ip", &["link", "set", bridge_name, "address", &parent_mac])
        .await
        .with_context(|| format!("Failed to set MAC on bridge {}", bridge_name))?;

    run_cmd("ip", &["link", "set", bridge_name, "up"])
        .await
        .with_context(|| format!("Failed to bring up bridge {}", bridge_name))?;

    // ── Migrate IP: parent → bridge ──────────────────────────────────────
    // Order matters: add IP to bridge first, then enslave parent.
    // Enslaving moves L3 processing to the bridge; having the IP already
    // there means zero window without an address.

    run_cmd("ip", &["addr", "add", &ip_cidr, "dev", bridge_name])
        .await
        .with_context(|| format!("Failed to add IP to bridge {}", bridge_name))?;

    run_cmd("ip", &["link", "set", parent_iface, "master", bridge_name])
        .await
        .with_context(|| format!("Failed to add {} to bridge {}", parent_iface, bridge_name))?;

    // Remove the (now-shadowed) IP from the parent port
    run_cmd("ip", &["addr", "del", &ip_cidr, "dev", parent_iface])
        .await
        .ok(); // may already be gone after enslave

    // ── Restore default route via bridge ─────────────────────────────────
    if let Some(ref gw) = default_gw {
        // The old route disappeared when eth0 was enslaved; re-add via bridge.
        run_cmd(
            "ip",
            &["route", "replace", "default", "via", gw, "dev", bridge_name],
        )
        .await
        .with_context(|| "Failed to restore default route")?;
    }

    // ── Sync networkd so it doesn't fight later ──────────────────────────
    if use_networkd {
        // State already matches the configs, so reload is non-disruptive.
        let br = bridge_name.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if let Err(e) = run_cmd("networkctl", &["reload"]).await {
                tracing::error!("Failed to reload networkd for bridge {}: {}", br, e);
            }
        });
    }

    info!(
        "Bridge {} created with parent {} (IP: {}, MAC: {})",
        bridge_name, parent_iface, ip_cidr, parent_mac
    );
    Ok(())
}

/// Check if systemd-networkd is active.
async fn is_networkd_running() -> bool {
    run_cmd("systemctl", &["is-active", "--quiet", "systemd-networkd"])
        .await
        .is_ok()
}

/// Write systemd-networkd configs for the bridge:
/// 1. A .netdev defining the bridge device
/// 2. A .network for the parent NIC (bridge member, no IP)
/// 3. A .network for the bridge (gets the IP + gateway)
///
/// These are written to /run/systemd/network/ (runtime, lost on reboot) to
/// avoid conflicting with persistent configs in /etc/systemd/network/.
async fn write_bridge_networkd_configs(
    bridge_name: &str,
    parent_iface: &str,
    ip_cidr: &str,
    gateway: Option<&str>,
) -> Result<()> {
    let dir = "/run/systemd/network";
    tokio::fs::create_dir_all(dir).await.ok();

    // High-priority prefix (05-) so these override default configs (10-)
    let netdev = format!("[NetDev]\nName={}\nKind=bridge\n", bridge_name);
    tokio::fs::write(format!("{}/05-{}.netdev", dir, bridge_name), &netdev).await?;

    let parent_net = format!(
        "[Match]\nName={}\n\n[Network]\nBridge={}\n",
        parent_iface, bridge_name
    );
    tokio::fs::write(
        format!("{}/05-{}-bridge-member.network", dir, parent_iface),
        &parent_net,
    )
    .await?;

    let mut bridge_net = format!(
        "[Match]\nName={}\n\n[Network]\nAddress={}\nDNS=8.8.8.8\n",
        bridge_name, ip_cidr
    );
    if let Some(gw) = gateway {
        bridge_net.push_str(&format!("Gateway={}\n", gw));
    }
    tokio::fs::write(format!("{}/05-{}.network", dir, bridge_name), &bridge_net).await?;

    Ok(())
}

/// Undo bridge_interface: move IP back from bridge to parent NIC, remove
/// parent from bridge, and delete the bridge.
pub async fn unbridge_interface(bridge_name: &str) -> Result<()> {
    info!("Unbridging {}", bridge_name);

    // Find the physical NIC member of this bridge
    let parent_iface = find_bridge_member(bridge_name).await?;

    // Clean up networkd runtime configs if present
    let dir = "/run/systemd/network";
    let _ = tokio::fs::remove_file(format!("{}/05-{}.netdev", dir, bridge_name)).await;
    let _ =
        tokio::fs::remove_file(format!("{}/05-{}-bridge-member.network", dir, parent_iface)).await;
    let _ = tokio::fs::remove_file(format!("{}/05-{}.network", dir, bridge_name)).await;

    if is_networkd_running().await {
        // Delete bridge, then let networkd reconfigure the parent NIC from its
        // persistent config in /etc/systemd/network/.
        run_cmd("ip", &["link", "del", bridge_name]).await.ok();
        run_cmd("networkctl", &["reload"]).await.ok();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    } else {
        // Manual path
        let addr_info = run_cmd_output("ip", &["-j", "addr", "show", bridge_name])
            .await
            .unwrap_or_default();
        let addrs: serde_json::Value =
            serde_json::from_str(&addr_info).unwrap_or(serde_json::json!([]));
        let ip_cidr = extract_ipv4_addr(&addrs).map(|(cidr, _)| cidr);

        let route_info = run_cmd_output("ip", &["-j", "route", "show", "default"])
            .await
            .unwrap_or_default();
        let default_gw = parse_default_gateway(&route_info, bridge_name);

        run_cmd("ip", &["link", "set", &parent_iface, "nomaster"])
            .await
            .ok();

        if let Some(ref cidr) = ip_cidr {
            run_cmd("ip", &["addr", "add", cidr, "dev", &parent_iface])
                .await
                .ok();
        }

        run_cmd("ip", &["link", "del", bridge_name]).await.ok();

        if let Some(gw) = default_gw {
            run_cmd(
                "ip",
                &["route", "add", "default", "via", &gw, "dev", &parent_iface],
            )
            .await
            .ok();
        }
    }

    info!("Unbridged {} (parent: {})", bridge_name, parent_iface);
    Ok(())
}

/// Check if a bridge has a physical NIC member (i.e., was created by bridge_interface).
pub async fn is_bridged_interface(bridge_name: &str) -> bool {
    find_bridge_member(bridge_name).await.is_ok()
}

/// Find a physical (non-virtual) member interface of a bridge.
async fn find_bridge_member(bridge_name: &str) -> Result<String> {
    let output = run_cmd_output("ip", &["-j", "link", "show", "master", bridge_name])
        .await
        .with_context(|| format!("Failed to list members of bridge {}", bridge_name))?;

    let links: serde_json::Value = serde_json::from_str(&output).unwrap_or(serde_json::json!([]));

    if let Some(arr) = links.as_array() {
        for link in arr {
            let ifname = link["ifname"].as_str().unwrap_or("");
            // Skip TAP and veth devices — look for physical NICs
            if !ifname.starts_with("tap") && !ifname.starts_with("veth") && !ifname.is_empty() {
                return Ok(ifname.to_string());
            }
        }
    }

    anyhow::bail!("No physical member found for bridge {}", bridge_name)
}

/// Extract the MAC address from `ip -j addr show` output.
fn extract_mac(addrs: &serde_json::Value) -> Option<String> {
    let arr = addrs.as_array()?;
    for iface in arr {
        if let Some(mac) = iface["address"].as_str()
            && mac.contains(':')
        {
            return Some(mac.to_string());
        }
    }
    None
}

/// Extract the first IPv4 address as "ip/prefix" from `ip -j addr show` output.
fn extract_ipv4_addr(addrs: &serde_json::Value) -> Option<(String, u32)> {
    let arr = addrs.as_array()?;
    for iface in arr {
        if let Some(addr_infos) = iface["addr_info"].as_array() {
            for ai in addr_infos {
                if ai["family"].as_str() == Some("inet") {
                    let local = ai["local"].as_str()?;
                    let prefixlen = ai["prefixlen"].as_u64().unwrap_or(24) as u32;
                    return Some((format!("{}/{}", local, prefixlen), prefixlen));
                }
            }
        }
    }
    None
}

/// Parse the default gateway from `ip -j route show default` if it goes via a given device.
fn parse_default_gateway(route_json: &str, dev: &str) -> Option<String> {
    let routes: serde_json::Value = serde_json::from_str(route_json).ok()?;
    let arr = routes.as_array()?;
    for route in arr {
        if route["dev"].as_str() == Some(dev) {
            return route["gateway"].as_str().map(|s| s.to_string());
        }
    }
    None
}

async fn run_cmd_output(program: &str, args: &[&str]) -> Result<String> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .with_context(|| format!("Failed to execute: {} {}", program, args.join(" ")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{} {} failed: {}", program, args.join(" "), stderr.trim())
    }
}

async fn run_cmd(program: &str, args: &[&str]) -> Result<()> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .with_context(|| format!("Failed to execute: {} {}", program, args.join(" ")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{} {} failed: {}", program, args.join(" "), stderr.trim())
    }
}
