/// Background task that periodically reconciles VM status between the database
/// and the live state reported by qarax-node. Detects drift caused by node
/// restarts or unexpected VM terminations and updates the DB accordingly.
use std::sync::Arc;

use sqlx::PgPool;
use tokio::time::{Duration, interval};
use tracing::{info, warn};

use crate::grpc_client::NodeClient;
use crate::model::vms::{self, VmStatus};

pub async fn start_vm_monitor(pool: Arc<PgPool>, node_address: String) {
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

        let client = NodeClient::from_address(&node_address);

        for vm in active_vms {
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
                    // Check whether the node reports the VM as not found (lost after restart)
                    // versus a connection error (node temporarily down).
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
                            "VM monitor: failed to get VM {} info (node may be down): {}",
                            vm.id, e
                        );
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
