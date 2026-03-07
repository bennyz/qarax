use anyhow::{Context, Result};
use tracing::info;

/// Set up NAT masquerade and forwarding rules for a bridge subnet.
pub async fn setup_nat(bridge: &str, subnet: &str) -> Result<()> {
    super::validate_iface_name(bridge)?;
    info!("Setting up NAT for bridge {} subnet {}", bridge, subnet);

    // Enable IP forwarding via /proc (sysctl binary may not be present)
    tokio::fs::write("/proc/sys/net/ipv4/ip_forward", b"1")
        .await
        .context("Failed to enable ip_forward")?;

    // Add masquerade rule for the subnet
    let _ = run_cmd(
        "iptables",
        &[
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-s",
            subnet,
            "!",
            "-o",
            bridge,
            "-j",
            "MASQUERADE",
        ],
    )
    .await;

    // Allow forwarding from/to the bridge
    let _ = run_cmd("iptables", &["-A", "FORWARD", "-i", bridge, "-j", "ACCEPT"]).await;

    let _ = run_cmd("iptables", &["-A", "FORWARD", "-o", bridge, "-j", "ACCEPT"]).await;

    Ok(())
}

/// Remove NAT and forwarding rules for a bridge subnet.
pub async fn teardown_nat(bridge: &str, subnet: &str) -> Result<()> {
    super::validate_iface_name(bridge)?;
    super::validate_ipv4_cidr(subnet)?;
    info!("Tearing down NAT for bridge {} subnet {}", bridge, subnet);

    let _ = run_cmd(
        "iptables",
        &[
            "-t",
            "nat",
            "-D",
            "POSTROUTING",
            "-s",
            subnet,
            "!",
            "-o",
            bridge,
            "-j",
            "MASQUERADE",
        ],
    )
    .await;

    let _ = run_cmd("iptables", &["-D", "FORWARD", "-i", bridge, "-j", "ACCEPT"]).await;

    let _ = run_cmd("iptables", &["-D", "FORWARD", "-o", bridge, "-j", "ACCEPT"]).await;

    Ok(())
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
