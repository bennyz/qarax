use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use edge_dhcp::server::{Server, ServerOptions};
use edge_dhcp::{Options, Packet};
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

const DHCP_SERVER_PORT: u16 = 67;
const DHCP_CLIENT_PORT: u16 = 68;
const MAX_LEASES: usize = 256;
const LEASE_DURATION_SECS: u32 = 43200; // 12 hours

static SERVERS: LazyLock<Mutex<HashMap<String, JoinHandle<()>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Start an in-process DHCP server on the specified bridge interface.
pub async fn start_dhcp_server(
    bridge: &str,
    range_start: &str,
    range_end: &str,
    gateway: &str,
    dns: &str,
) -> Result<()> {
    info!(
        "Starting DHCP server on {bridge} (range {range_start}-{range_end}, gw={gateway}, dns={dns})"
    );

    // Validate inputs before any side effects (stopping existing server, binding socket)
    let gateway_ip: Ipv4Addr = gateway.parse().context("Invalid gateway IP")?;
    let range_start_ip: Ipv4Addr = range_start.parse().context("Invalid DHCP range start IP")?;
    let range_end_ip: Ipv4Addr = range_end.parse().context("Invalid DHCP range end IP")?;
    let dns_ip: Ipv4Addr = dns.parse().context("Invalid DNS IP")?;

    // Hold lock across stop + socket bind + insert to prevent races between
    // concurrent start_dhcp_server calls for the same bridge.
    let mut servers = SERVERS.lock().unwrap();

    if let Some(handle) = servers.remove(bridge) {
        info!("Stopping existing DHCP server for bridge {bridge}");
        handle.abort();
    }

    let socket = create_dhcp_socket(bridge)?;
    let bridge_key = bridge.to_string();
    let bridge_name = bridge_key.clone();

    let handle = tokio::spawn(async move {
        if let Err(e) = run_dhcp_loop(
            socket,
            gateway_ip,
            range_start_ip,
            range_end_ip,
            dns_ip,
            &bridge_name,
        )
        .await
        {
            tracing::error!("DHCP server for {bridge_name} exited with error: {e}");
        }
    });

    servers.insert(bridge_key, handle);
    debug!("DHCP server started for bridge {bridge}");
    Ok(())
}

/// Stop the DHCP server for a bridge.
pub async fn stop_dhcp_server(bridge: &str) -> Result<()> {
    if let Some(handle) = SERVERS.lock().unwrap().remove(bridge) {
        info!("Stopping DHCP server for bridge {bridge}");
        handle.abort();
    }
    Ok(())
}

fn create_dhcp_socket(bridge: &str) -> Result<UdpSocket> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .context("Failed to create UDP socket")?;

    socket
        .set_reuse_address(true)
        .context("Failed to set SO_REUSEADDR")?;
    socket
        .set_broadcast(true)
        .context("Failed to set SO_BROADCAST")?;
    socket
        .bind_device(Some(bridge.as_bytes()))
        .context("Failed to bind socket to bridge interface")?;
    socket
        .bind(&socket2::SockAddr::from(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            DHCP_SERVER_PORT,
        )))
        .context("Failed to bind to DHCP server port")?;
    socket
        .set_nonblocking(true)
        .context("Failed to set socket non-blocking")?;

    UdpSocket::from_std(socket.into()).context("Failed to create tokio UdpSocket")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn run_dhcp_loop(
    socket: UdpSocket,
    gateway: Ipv4Addr,
    range_start: Ipv4Addr,
    range_end: Ipv4Addr,
    dns: Ipv4Addr,
    bridge: &str,
) -> Result<()> {
    let mut server = Server::<_, MAX_LEASES>::new(now_secs, gateway);
    server.range_start = range_start;
    server.range_end = range_end;

    let gw_buf = [gateway];
    let dns_buf = [dns];
    let mut options = ServerOptions::new(gateway, None);
    options.gateways = &gw_buf;
    options.dns = &dns_buf;
    options.lease_duration_secs = LEASE_DURATION_SECS;

    info!("DHCP server for {bridge}: range {range_start}-{range_end}, gw={gateway}, dns={dns}");

    let mut buf = [0u8; 1500];

    loop {
        let (len, _src) = socket.recv_from(&mut buf).await?;

        let request = match Packet::decode(&buf[..len]) {
            Ok(pkt) => pkt,
            Err(e) => {
                debug!("Invalid DHCP packet on {bridge}: {e:?}");
                continue;
            }
        };

        let mut opt_buf = Options::buf();

        if let Some(reply) = server.handle_request(&mut opt_buf, &options, &request) {
            // RFC 2131 Section 4.1 destination rules
            let (dest_ip, dest_port) = if !request.giaddr.is_unspecified() {
                (request.giaddr, DHCP_SERVER_PORT)
            } else if !request.ciaddr.is_unspecified() && !request.broadcast {
                (request.ciaddr, DHCP_CLIENT_PORT)
            } else {
                (Ipv4Addr::BROADCAST, DHCP_CLIENT_PORT)
            };

            match reply.encode(&mut buf) {
                Ok(encoded) => {
                    let dest = SocketAddrV4::new(dest_ip, dest_port);
                    if let Err(e) = socket.send_to(encoded, dest).await {
                        warn!("Failed to send DHCP reply on {bridge}: {e}");
                    }
                }
                Err(e) => warn!("Failed to encode DHCP reply on {bridge}: {e:?}"),
            }
        }
    }
}
