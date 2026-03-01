use anyhow::{Context, Result};
use tracing::{debug, info, warn};

const PID_DIR: &str = "/var/run";

/// Start a dnsmasq instance bound to the specified bridge interface.
pub async fn start_dnsmasq(
    bridge: &str,
    range_start: &str,
    range_end: &str,
    gateway: &str,
    dns: &str,
) -> Result<()> {
    info!(
        "Starting dnsmasq on {} (range {}-{}, gw={}, dns={})",
        bridge, range_start, range_end, gateway, dns
    );

    let pid_file = pid_file_path(bridge);

    // Kill any existing dnsmasq for this bridge first
    let _ = stop_dnsmasq(bridge).await;

    let output = tokio::process::Command::new("dnsmasq")
        .args([
            "--strict-order",
            "--bind-interfaces",
            &format!("--interface={}", bridge),
            &format!("--dhcp-range={},{},12h", range_start, range_end),
            &format!("--dhcp-option=option:router,{}", gateway),
            &format!("--dhcp-option=option:dns-server,{}", dns),
            "--except-interface=lo",
            "--no-resolv",
            "--no-hosts",
            &format!("--pid-file={}", pid_file),
            "--log-dhcp",
        ])
        .output()
        .await
        .context("Failed to start dnsmasq")?;

    if output.status.success() {
        debug!("dnsmasq started for bridge {}", bridge);
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("dnsmasq failed to start: {}", stderr.trim())
    }
}

/// Stop the dnsmasq instance for a bridge by reading its PID file.
pub async fn stop_dnsmasq(bridge: &str) -> Result<()> {
    let pid_file = pid_file_path(bridge);

    match tokio::fs::read_to_string(&pid_file).await {
        Ok(content) => {
            let pid = content.trim();
            if !pid.is_empty() {
                info!("Stopping dnsmasq for bridge {} (PID {})", bridge, pid);
                let _ = tokio::process::Command::new("kill").arg(pid).output().await;
            }
            let _ = tokio::fs::remove_file(&pid_file).await;
            Ok(())
        }
        Err(_) => {
            warn!("No PID file found for dnsmasq on bridge {}", bridge);
            Ok(())
        }
    }
}

fn pid_file_path(bridge: &str) -> String {
    format!("{}/qarax-dnsmasq-{}.pid", PID_DIR, bridge)
}
