/// Background task that periodically polls resource metrics from UP hosts
/// and persists them in the database for resource-aware scheduling.
use std::sync::Arc;

use sqlx::PgPool;
use tokio::time::{Duration, interval};
use tracing::warn;

use crate::grpc_client::NodeClient;
use crate::model::hosts;

pub async fn start_resource_monitor(pool: Arc<PgPool>) {
    let mut ticker = interval(Duration::from_secs(10));

    loop {
        ticker.tick().await;

        let up_hosts = match hosts::list_up(&pool).await {
            Ok(h) => h,
            Err(e) => {
                warn!("Resource monitor: failed to list UP hosts: {}", e);
                continue;
            }
        };

        if up_hosts.is_empty() {
            continue;
        }

        for host in up_hosts {
            let client = NodeClient::new(&host.address, host.port as u16);

            match client.get_node_info().await {
                Ok(info) => {
                    if let Err(e) = hosts::update_resources(
                        &pool,
                        host.id,
                        info.total_cpus,
                        info.total_memory_bytes,
                        info.available_memory_bytes,
                        info.load_average_1m,
                        info.disk_total_bytes,
                        info.disk_available_bytes,
                    )
                    .await
                    {
                        warn!(
                            "Resource monitor: failed to update resources for host {}: {}",
                            host.name, e
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "Resource monitor: failed to get node info from host {} (may be down): {}",
                        host.name, e
                    );
                }
            }
        }
    }
}
