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
        anyhow::bail!(
            "{} {} failed: {}",
            program,
            args.join(" "),
            stderr.trim()
        )
    }
}
