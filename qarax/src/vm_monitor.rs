/// Background task that periodically reconciles VM status between the database
/// and the live state reported by qarax-node. Detects drift caused by node
/// restarts or unexpected VM terminations and updates the DB accordingly.
use std::collections::HashMap;
use std::sync::Arc;

use sqlx::PgPool;
use tokio::time::{Duration, interval};
use tracing::{info, warn};
use uuid::Uuid;

use crate::grpc_client::NodeClient;
use crate::model::{
    hosts,
    vms::{self, VmStatus},
};

pub async fn start_vm_monitor(pool: Arc<PgPool>) {
    let mut ticker = interval(Duration::from_secs(30));

    loop {
        ticker.tick().await;

        let active_vms = match vms::list_active(&pool).await {
            Ok(vms) => vms,
            Err(e) => {
                warn!("VM monitor: failed to list active VMs: {}", e);
                continue;
            }
        };

        if active_vms.is_empty() {
            continue;
        }

        // Group VMs by host so we open one gRPC connection per host per tick
        let mut by_host: HashMap<Uuid, Vec<_>> = HashMap::new();
        for vm in active_vms {
            if let Some(host_id) = vm.host_id {
                by_host.entry(host_id).or_default().push(vm);
            }
        }

        for (host_id, vms) in by_host {
            let host = match hosts::get_by_id(&pool, host_id).await {
                Ok(Some(h)) => h,
                Ok(None) => {
                    warn!("VM monitor: host {} not found in DB", host_id);
                    continue;
                }
                Err(e) => {
                    warn!("VM monitor: failed to look up host {}: {}", host_id, e);
                    continue;
                }
            };

            let client = NodeClient::new(&host.address, host.port as u16);

            for vm in vms {
                match client.get_vm_info(vm.id).await {
                    Ok(state) => {
                        let live_status = proto_status_to_db(state.status);
                        if live_status != vm.status {
                            info!(
                                "VM monitor: VM {} status changed from {:?} to {:?}",
                                vm.id, vm.status, live_status
                            );
                            if let Err(e) = vms::update_status(&pool, vm.id, live_status).await {
                                warn!("VM monitor: failed to update VM {} status: {}", vm.id, e);
                            }
                        }
                    }
                    Err(e) => {
                        let e_str = format!("{}", e);
                        if e_str.to_lowercase().contains("not found") {
                            info!(
                                "VM monitor: VM {} not found on node, marking as Unknown",
                                vm.id
                            );
                            if let Err(db_err) =
                                vms::update_status(&pool, vm.id, VmStatus::Unknown).await
                            {
                                warn!(
                                    "VM monitor: failed to update VM {} status: {}",
                                    vm.id, db_err
                                );
                            }
                        } else {
                            warn!(
                                "VM monitor: failed to get VM {} info from host {} (node may be down): {}",
                                vm.id, host.name, e
                            );
                        }
                    }
                }
            }
        }
    }
}

fn proto_status_to_db(status: i32) -> VmStatus {
    // Proto VmStatus values:
    // VM_STATUS_UNKNOWN = 0, VM_STATUS_CREATED = 1, VM_STATUS_RUNNING = 2,
    // VM_STATUS_PAUSED = 3, VM_STATUS_SHUTDOWN = 4
    match status {
        1 => VmStatus::Created,
        2 => VmStatus::Running,
        3 => VmStatus::Paused,
        4 => VmStatus::Shutdown,
        _ => VmStatus::Unknown,
    }
}
