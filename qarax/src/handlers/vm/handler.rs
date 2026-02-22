use std::collections::HashMap;

use axum::{Extension, Json, extract::Path, response::IntoResponse};
use http::StatusCode;
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    App,
    grpc_client::{CreateVmRequest, NodeClient, net_configs_from_api, node::FsConfig},
    model::{
        boot_sources, hosts,
        hosts::Host,
        jobs::{self, JobType, NewJob},
        network_interfaces,
        vm_filesystems::{self, NewVmFilesystem},
        vms::{self, NewVm, Vm, VmStatus},
    },
};

use super::{ApiResponse, Result};

#[derive(Serialize, ToSchema)]
pub struct CreateVmResponse {
    pub vm_id: Uuid,
    pub job_id: Uuid,
}

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
        (status = 201, description = "VM created successfully (synchronous)", body = String, content_type = "application/json"),
        (status = 202, description = "VM creation started asynchronously", body = CreateVmResponse),
        (status = 422, description = "Invalid input"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn create(
    Extension(env): Extension<App>,
    Json(vm): Json<NewVm>,
) -> Result<axum::response::Response> {
    // If an OCI image_ref is provided, use the async job path
    if vm.image_ref.is_some() {
        return create_with_image(env, vm).await;
    }

    // Synchronous path (no image_ref)
    let mut tx = env.pool().begin().await?;
    let id = vms::create_tx(&mut tx, &vm).await?;

    // Resolve boot source or use defaults
    let (kernel, initramfs, cmdline) = if let Some(boot_source_id) = vm.boot_source_id {
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
        let vm_defaults = env.vm_defaults();
        (
            vm_defaults.kernel.clone(),
            vm_defaults.initramfs.clone(),
            vm_defaults.cmdline.clone(),
        )
    };

    // Pick a host before touching the node, so we fail fast with a clear error
    let host = pick_host(&env).await?;
    let node_client = NodeClient::new(&host.address, host.port as u16);

    let memory_shared = vm.memory_shared.unwrap_or(false);
    let networks = net_configs_from_api(vm.networks.as_deref().unwrap_or(&[]));
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
            fs_configs: vec![],
            memory_shared,
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

    Ok(ApiResponse {
        data: id.to_string(),
        code: StatusCode::CREATED,
    }
    .into_response())
}

/// Async path: pull OCI image and create VM in a background job, return 202 immediately.
async fn create_with_image(env: App, vm: NewVm) -> Result<axum::response::Response> {
    let image_ref = vm
        .image_ref
        .clone()
        .expect("image_ref checked before calling");

    // Pick host eagerly so we return 422 immediately if none are UP
    let host = pick_host(&env).await?;

    // Resolve boot source or use defaults (needed by background task)
    let (kernel, initramfs, _cmdline) = if let Some(boot_source_id) = vm.boot_source_id {
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
        let vm_defaults = env.vm_defaults();
        (
            vm_defaults.kernel.clone(),
            vm_defaults.initramfs.clone(),
            vm_defaults.cmdline.clone(),
        )
    };

    // Create VM row with PENDING status
    let mut tx = env.pool().begin().await?;
    let vm_id = vms::create_tx_with_status(&mut tx, &vm, VmStatus::Pending).await?;

    // Store network interfaces in the transaction
    for net in vm.networks.as_deref().unwrap_or(&[]) {
        network_interfaces::create(&mut tx, vm_id, net)
            .await
            .map_err(crate::errors::Error::Sqlx)?;
    }

    tx.commit().await?;

    // Record host assignment
    let _ = vms::update_host_id(env.pool(), vm_id, host.id).await;

    // Create job record
    let job = jobs::create(
        env.pool(),
        NewJob {
            job_type: JobType::ImagePull,
            description: Some(format!("Pulling image {}", image_ref)),
            resource_id: Some(vm_id),
            resource_type: Some("vm".to_string()),
        },
    )
    .await?;
    let job_id = job.id;

    // Spawn background task
    let db_pool = env.pool_arc();
    let networks = net_configs_from_api(vm.networks.as_deref().unwrap_or(&[]));

    tokio::spawn(async move {
        tracing::info!(vm_id = %vm_id, job_id = %job_id, image_ref = %image_ref, "Starting async OCI image pull");

        if let Err(e) = jobs::mark_running(&db_pool, job_id).await {
            tracing::error!(job_id = %job_id, error = %e, "Failed to mark job as running");
            return;
        }

        let node_client = NodeClient::new(&host.address, host.port as u16);

        // Step 1: Pull image
        let image_info = match node_client.pull_image(&image_ref).await {
            Ok(info) => info,
            Err(e) => {
                let msg = format!("Failed to pull OCI image: {}", e);
                tracing::error!(vm_id = %vm_id, job_id = %job_id, error = %msg);
                let _ = jobs::mark_failed(&db_pool, job_id, &msg).await;
                let _ = vms::update_status(&db_pool, vm_id, VmStatus::Unknown).await;
                return;
            }
        };

        let _ = jobs::update_progress(&db_pool, job_id, 50).await;

        // Step 2: Create VM on node with virtiofs config
        let socket_path = format!("/var/lib/qarax/vms/{}-fs0.sock", vm_id);
        let fs_config = FsConfig {
            tag: "rootfs".to_string(),
            socket: socket_path,
            num_queues: 1,
            queue_size: 1024,
            pci_segment: None,
            id: Some("fs0".to_string()),
            bootstrap_path: Some(image_info.bootstrap_path.clone()),
        };
        let oci_cmdline =
            "console=ttyS0 root=rootfs rootfstype=virtiofs rw init=/.qarax-init".to_string();

        if let Err(e) = node_client
            .create_vm(CreateVmRequest {
                vm_id,
                boot_vcpus: vm.boot_vcpus,
                max_vcpus: vm.max_vcpus,
                memory_size: vm.memory_size,
                networks,
                kernel,
                initramfs,
                cmdline: oci_cmdline,
                fs_configs: vec![fs_config],
                memory_shared: true,
            })
            .await
        {
            let msg = format!("Failed to create VM on qarax-node: {}", e);
            tracing::error!(vm_id = %vm_id, job_id = %job_id, error = %msg);
            let _ = jobs::mark_failed(&db_pool, job_id, &msg).await;
            let _ = vms::update_status(&db_pool, vm_id, VmStatus::Unknown).await;
            return;
        }

        // Step 3: Persist virtiofs filesystem record
        let fs_record = NewVmFilesystem {
            vm_id,
            tag: "rootfs".to_string(),
            num_queues: Some(1),
            queue_size: Some(1024),
            pci_segment: None,
            image_ref: Some(image_ref.clone()),
            image_digest: Some(image_info.digest.clone()),
        };
        if let Err(e) = vm_filesystems::create(&db_pool, &fs_record).await {
            tracing::warn!(vm_id = %vm_id, error = %e, "Failed to persist filesystem record");
        }

        // Mark VM as created and job as completed
        let _ = vms::update_status(&db_pool, vm_id, VmStatus::Created).await;
        let result = serde_json::json!({ "digest": image_info.digest });
        let _ = jobs::mark_completed(&db_pool, job_id, Some(result)).await;

        tracing::info!(vm_id = %vm_id, job_id = %job_id, "VM creation job completed");
    });

    use axum::response::IntoResponse as _;
    Ok(ApiResponse {
        data: CreateVmResponse { vm_id, job_id },
        code: StatusCode::ACCEPTED,
    }
    .into_response())
}

#[utoipa::path(
    post,
    path = "/vms/{vm_id}/start",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 202, description = "VM start accepted"),
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
    let vm = vms::get(env.pool(), vm_id).await?;

    match vm.status {
        VmStatus::Running => {
            return Ok(ApiResponse {
                data: (),
                code: StatusCode::ACCEPTED,
            });
        }
        VmStatus::Paused => {
            return Err(crate::errors::Error::UnprocessableEntity(
                "VM is paused; use POST /vms/{vm_id}/resume instead".into(),
            ));
        }
        VmStatus::Pending => {
            return Err(crate::errors::Error::UnprocessableEntity(
                "VM is pending job completion; wait for the job to finish".into(),
            ));
        }
        VmStatus::Created | VmStatus::Shutdown | VmStatus::Unknown => {
            // Valid states to start from
        }
    }

    let host = host_for_vm(&env, vm_id).await?;
    let node_client = NodeClient::new(&host.address, host.port as u16);
    node_client.start_vm(vm_id).await.map_err(|e| {
        tracing::error!("Failed to start VM on qarax-node: {}", e);
        crate::errors::Error::InternalServerError
    })?;

    vms::update_status(env.pool(), vm_id, VmStatus::Running).await?;

    Ok(ApiResponse {
        data: (),
        code: StatusCode::ACCEPTED,
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
    let vm = vms::get(env.pool(), vm_id).await?;

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

#[utoipa::path(
    get,
    path = "/vms/{vm_id}/console",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 200, description = "Console log content", body = String, content_type = "text/plain"),
        (status = 404, description = "Console logging not available for this VM"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn console_log(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
) -> Result<axum::response::Response> {
    let host = host_for_vm(&env, vm_id).await?;
    let node_client = NodeClient::new(&host.address, host.port as u16);

    let response = node_client.read_console_log(vm_id).await.map_err(|e| {
        tracing::error!("Failed to read console log: {}", e);
        crate::errors::Error::InternalServerError
    })?;

    if !response.available {
        return Err(crate::errors::Error::NotFound);
    }

    Ok(axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(axum::body::Body::from(response.content))
        .unwrap())
}
