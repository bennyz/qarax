use std::collections::HashMap;

use axum::{Extension, Json, extract::Path};
use http::StatusCode;
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    App,
    grpc_client::{CreateVmRequest, NodeClient, net_configs_from_api},
    model::{
        boot_sources, hosts,
        hosts::Host,
        network_interfaces,
        vms::{self, NewVm, Vm, VmStatus},
    },
};

use super::{ApiResponse, Result};

#[derive(Serialize, ToSchema)]
pub struct VmMetrics {
    pub vm_id: Uuid,
    pub status: VmStatus,
    pub memory_actual_size: Option<i64>,
    pub counters: HashMap<String, HashMap<String, i64>>,
}

/// Pick a random UP host for scheduling a new VM.
async fn pick_host(env: &App) -> Result<Host> {
    hosts::pick_up_host(env.pool()).await?.ok_or_else(|| {
        crate::errors::Error::UnprocessableEntity(
            "no hosts in UP state available for scheduling".into(),
        )
    })
}

/// Resolve the host that a VM is assigned to, for routing subsequent operations.
async fn host_for_vm(env: &App, vm_id: Uuid) -> Result<Host> {
    let vm = vms::get(env.pool(), vm_id).await?;
    let host_id = vm.host_id.ok_or_else(|| {
        crate::errors::Error::UnprocessableEntity("VM has no assigned host".into())
    })?;
    hosts::get_by_id(env.pool(), host_id)
        .await?
        .ok_or_else(|| crate::errors::Error::UnprocessableEntity("assigned host not found".into()))
}

#[utoipa::path(
    get,
    path = "/vms",
    responses(
        (status = 200, description = "List all VMs", body = Vec<Vm>),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn list(Extension(env): Extension<App>) -> Result<ApiResponse<Vec<Vm>>> {
    let hosts = vms::list(env.pool()).await?;
    Ok(ApiResponse {
        data: hosts,
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    get,
    path = "/vms/{vm_id}",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 200, description = "VM details", body = Vm),
        (status = 404, description = "VM not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn get(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
) -> Result<ApiResponse<Vm>> {
    let vm = vms::get(env.pool(), vm_id).await?;
    Ok(ApiResponse {
        data: vm,
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    post,
    path = "/vms",
    request_body = NewVm,
    responses(
        (status = 201, description = "VM created successfully", body = String),
        (status = 422, description = "Invalid input"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn create(
    Extension(env): Extension<App>,
    Json(vm): Json<NewVm>,
) -> Result<(StatusCode, String)> {
    let mut tx = env.pool().begin().await?;
    let id = vms::create_tx(&mut tx, &vm).await?;

    // Resolve boot source or use defaults
    let (kernel, initramfs, cmdline) = if let Some(boot_source_id) = vm.boot_source_id {
        // Resolve boot source
        let resolved = boot_sources::resolve(env.pool(), boot_source_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to resolve boot_source_id {}: {}", boot_source_id, e);
                crate::errors::Error::UnprocessableEntity(format!(
                    "Invalid boot_source_id: {}",
                    boot_source_id
                ))
            })?;

        (
            resolved.kernel_path,
            resolved.initramfs_path,
            resolved.kernel_params,
        )
    } else {
        // Fall back to vm_defaults
        let vm_defaults = env.vm_defaults();
        (
            vm_defaults.kernel.clone(),
            vm_defaults.initramfs.clone(),
            vm_defaults.cmdline.clone(),
        )
    };

    // Pick a host before touching the node, so we fail fast with a clear error
    let host = pick_host(&env).await?;

    // Call qarax-node to create the VM; on failure we return before commit so the insert is rolled back
    let networks = net_configs_from_api(vm.networks.as_deref().unwrap_or(&[]));
    let node_client = NodeClient::new(&host.address, host.port as u16);
    if let Err(e) = node_client
        .create_vm(CreateVmRequest {
            vm_id: id,
            boot_vcpus: vm.boot_vcpus,
            max_vcpus: vm.max_vcpus,
            memory_size: vm.memory_size,
            networks,
            kernel,
            initramfs,
            cmdline,
        })
        .await
    {
        tracing::error!("Failed to create VM on qarax-node: {}", e);
        return Err(crate::errors::Error::UnprocessableEntity(format!(
            "qarax-node: {}",
            e
        )));
    }

    // Store network interfaces in DB (inside tx, so rolls back if any insert fails)
    for net in vm.networks.as_deref().unwrap_or(&[]) {
        network_interfaces::create(&mut tx, id, net)
            .await
            .map_err(crate::errors::Error::Sqlx)?;
    }

    tx.commit().await?;

    // Record which host this VM was scheduled onto
    let _ = vms::update_host_id(env.pool(), id, host.id).await;

    Ok((StatusCode::CREATED, id.to_string()))
}

#[utoipa::path(
    post,
    path = "/vms/{vm_id}/start",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 200, description = "VM started successfully"),
        (status = 404, description = "VM not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn start(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
) -> Result<ApiResponse<()>> {
    let host = host_for_vm(&env, vm_id).await?;
    let node_client = NodeClient::new(&host.address, host.port as u16);
    node_client.start_vm(vm_id).await.map_err(|e| {
        tracing::error!("Failed to start VM on qarax-node: {}", e);
        crate::errors::Error::InternalServerError
    })?;

    vms::update_status(env.pool(), vm_id, VmStatus::Running).await?;

    Ok(ApiResponse {
        data: (),
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    post,
    path = "/vms/{vm_id}/stop",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 200, description = "VM stopped successfully"),
        (status = 404, description = "VM not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn stop(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
) -> Result<ApiResponse<()>> {
    let host = host_for_vm(&env, vm_id).await?;
    let node_client = NodeClient::new(&host.address, host.port as u16);
    node_client.stop_vm(vm_id).await.map_err(|e| {
        tracing::error!("Failed to stop VM on qarax-node: {}", e);
        crate::errors::Error::InternalServerError
    })?;

    vms::update_status(env.pool(), vm_id, VmStatus::Shutdown).await?;

    Ok(ApiResponse {
        data: (),
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    post,
    path = "/vms/{vm_id}/pause",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 200, description = "VM paused successfully"),
        (status = 404, description = "VM not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn pause(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
) -> Result<ApiResponse<()>> {
    let host = host_for_vm(&env, vm_id).await?;
    let node_client = NodeClient::new(&host.address, host.port as u16);
    node_client.pause_vm(vm_id).await.map_err(|e| {
        tracing::error!("Failed to pause VM on qarax-node: {}", e);
        crate::errors::Error::InternalServerError
    })?;

    vms::update_status(env.pool(), vm_id, VmStatus::Paused).await?;

    Ok(ApiResponse {
        data: (),
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    post,
    path = "/vms/{vm_id}/resume",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 200, description = "VM resumed successfully"),
        (status = 404, description = "VM not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn resume(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
) -> Result<ApiResponse<()>> {
    let host = host_for_vm(&env, vm_id).await?;
    let node_client = NodeClient::new(&host.address, host.port as u16);
    node_client.resume_vm(vm_id).await.map_err(|e| {
        tracing::error!("Failed to resume VM on qarax-node: {}", e);
        crate::errors::Error::InternalServerError
    })?;

    vms::update_status(env.pool(), vm_id, VmStatus::Running).await?;

    Ok(ApiResponse {
        data: (),
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    get,
    path = "/vms/{vm_id}/metrics",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 200, description = "VM live metrics and counters", body = VmMetrics),
        (status = 404, description = "VM not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn metrics(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
) -> Result<ApiResponse<VmMetrics>> {
    // Verify VM exists in DB and resolve its host
    let vm = match vms::get(env.pool(), vm_id).await {
        Ok(vm) => vm,
        Err(sqlx::Error::RowNotFound) => return Err(crate::errors::Error::NotFound),
        Err(e) => return Err(crate::errors::Error::Sqlx(e)),
    };

    let host = host_for_vm(&env, vm_id).await?;
    let node_client = NodeClient::new(&host.address, host.port as u16);

    // Get live status and memory info from node
    let (status, memory_actual_size) = match node_client.get_vm_info(vm_id).await {
        Ok(state) => {
            let live_status = match state.status {
                1 => VmStatus::Created,
                2 => VmStatus::Running,
                3 => VmStatus::Paused,
                4 => VmStatus::Shutdown,
                _ => VmStatus::Unknown,
            };
            (live_status, state.memory_actual_size)
        }
        Err(_) => {
            // Node unreachable or VM not found on node — return DB status
            (vm.status, None)
        }
    };

    // Get live counters from node
    let counters = match node_client.get_vm_counters(vm_id).await {
        Ok(c) => c
            .counters
            .into_iter()
            .map(|(device, device_counters)| (device, device_counters.values))
            .collect(),
        Err(_) => HashMap::new(),
    };

    Ok(ApiResponse {
        data: VmMetrics {
            vm_id,
            status,
            memory_actual_size,
            counters,
        },
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    delete,
    path = "/vms/{vm_id}",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 204, description = "VM deleted successfully"),
        (status = 404, description = "VM not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn delete(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
) -> Result<ApiResponse<()>> {
    let host = host_for_vm(&env, vm_id).await?;
    let node_client = NodeClient::new(&host.address, host.port as u16);
    node_client.delete_vm(vm_id).await.map_err(|e| {
        tracing::error!("Failed to delete VM on qarax-node: {}", e);
        crate::errors::Error::InternalServerError
    })?;

    vms::delete(env.pool(), vm_id).await?;

    Ok(ApiResponse {
        data: (),
        code: StatusCode::NO_CONTENT,
    })
}
