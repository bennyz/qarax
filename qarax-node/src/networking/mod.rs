pub mod bridge;
pub mod dhcp;
pub mod nftables;

/// Validate a network interface name.
///
/// Enforces kernel rules (IFNAMSIZ=16, no '/' or NUL) plus additional
/// restrictions needed when names appear in file paths (path traversal),
/// systemd.network files (whitespace breaks ini parsing), or are passed to
/// command-line tools like iptables (leading '-' is misread as a flag).
pub(super) fn validate_iface_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() || name.len() > 15 {
        anyhow::bail!("Interface name {name:?} has invalid length (must be 1–15 chars)");
    }
    if name.starts_with('-') {
        anyhow::bail!("Interface name {name:?} must not start with '-'");
    }
    if name
        .chars()
        .any(|c| c == '/' || c == '\0' || c.is_whitespace())
    {
        anyhow::bail!("Interface name {name:?} contains illegal characters");
    }
    Ok(())
}
