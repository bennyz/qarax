use std::collections::HashMap;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::{Extension, Json, extract::Path, response::IntoResponse};
use futures::{SinkExt, StreamExt};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{error, info, instrument, warn};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    App,
    grpc_client::{
        CreateVmRequest, NodeClient, OverlaybdDiskSpec, net_configs_from_db, node::FsConfig,
    },
    model::{
        boot_sources, hosts,
        hosts::Host,
        jobs::{self, JobType, NewJob},
        network_interfaces, storage_objects,
        storage_pools::{self, OverlayBdPoolConfig},
        vm_filesystems::{self, NewVmFilesystem},
        vm_overlaybd_disks::{self, NewVmOverlaybdDisk},
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
pub struct VmStartResponse {
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

    // Synchronous path (no image_ref): only write to DB, pick host, store networks.
    // The node is NOT contacted here. create_vm will be called lazily at vm start.
    let mut tx = env.pool().begin().await?;
    let id = vms::create_tx(&mut tx, &vm).await?;

    // Store network interfaces in DB (inside tx, so rolls back if any insert fails)
    for net in vm.networks.as_deref().unwrap_or(&[]) {
        network_interfaces::create(&mut tx, id, net)
            .await
            .map_err(crate::errors::Error::Sqlx)?;
    }

    tx.commit().await?;

    // Pick a host and record it
    let host = pick_host(&env).await?;
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

    // Check if the selected host has an OverlayBD storage pool; if so, use that path.
    let overlaybd_pool = storage_pools::find_overlaybd_for_host(env.pool(), host.id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query OverlayBD pool for host {}: {}", host.id, e);
            crate::errors::Error::InternalServerError
        })?;

    // Resolve boot source or use defaults (stored for use at start time)
    let (_kernel, _initramfs, _cmdline) = if let Some(boot_source_id) = vm.boot_source_id {
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

    tokio::spawn(async move {
        tracing::info!(vm_id = %vm_id, job_id = %job_id, image_ref = %image_ref, "Starting async OCI image pull");

        if let Err(e) = jobs::mark_running(&db_pool, job_id).await {
            tracing::error!(job_id = %job_id, error = %e, "Failed to mark job as running");
            return;
        }

        let node_client = NodeClient::new(&host.address, host.port as u16);

        if let Some(pool) = overlaybd_pool {
            // OverlayBD path: import image to local registry, then boot via virtio-blk
            run_overlaybd_create(
                &node_client,
                &db_pool,
                vm_id,
                job_id,
                &image_ref,
                &pool.config,
                pool.id,
            )
            .await;
        } else {
            // Virtiofs path: pull image via Nydus, serve rootfs via virtiofs
            run_virtiofs_create(&node_client, &db_pool, vm_id, job_id, &image_ref).await;
        }
    });

    use axum::response::IntoResponse as _;
    Ok(ApiResponse {
        data: CreateVmResponse { vm_id, job_id },
        code: StatusCode::ACCEPTED,
    }
    .into_response())
}

/// Background task for the virtiofs (Nydus) OCI image boot path.
#[allow(clippy::too_many_arguments)]
async fn run_virtiofs_create(
    node_client: &NodeClient,
    db_pool: &sqlx::PgPool,
    vm_id: uuid::Uuid,
    job_id: uuid::Uuid,
    image_ref: &str,
) {
    // Step 1: Pull image
    let image_info = match node_client.pull_image(image_ref).await {
        Ok(info) => info,
        Err(e) => {
            let msg = format!("Failed to pull OCI image: {}", e);
            tracing::error!(vm_id = %vm_id, job_id = %job_id, error = %msg);
            let _ = jobs::mark_failed(db_pool, job_id, &msg).await;
            let _ = vms::update_status(db_pool, vm_id, VmStatus::Unknown).await;
            return;
        }
    };

    let _ = jobs::update_progress(db_pool, job_id, 50).await;

    // Step 2: Persist virtiofs filesystem record
    let fs_record = NewVmFilesystem {
        vm_id,
        tag: "rootfs".to_string(),
        num_queues: Some(1),
        queue_size: Some(1024),
        pci_segment: None,
        image_ref: Some(image_ref.to_string()),
        image_digest: Some(image_info.digest.clone()),
    };
    if let Err(e) = vm_filesystems::create(db_pool, &fs_record).await {
        tracing::warn!(vm_id = %vm_id, error = %e, "Failed to persist filesystem record");
    }

    // Mark VM as created (on node provisioning is deferred to vm start)
    let _ = vms::update_status(db_pool, vm_id, VmStatus::Created).await;
    let result = serde_json::json!({ "digest": image_info.digest });
    let _ = jobs::mark_completed(db_pool, job_id, Some(result)).await;

    tracing::info!(vm_id = %vm_id, job_id = %job_id, "VM creation job completed (virtiofs)");
}

/// Background task for the OverlayBD lazy block loading path.
#[allow(clippy::too_many_arguments)]
async fn run_overlaybd_create(
    node_client: &NodeClient,
    db_pool: &sqlx::PgPool,
    vm_id: uuid::Uuid,
    job_id: uuid::Uuid,
    image_ref: &str,
    pool_config: &serde_json::Value,
    storage_pool_id: uuid::Uuid,
) {
    // Extract registry URL from pool config
    let registry_url = match OverlayBdPoolConfig::from_value(pool_config) {
        Some(cfg) => cfg.url,
        None => {
            let msg = "OverlayBD pool config missing 'url' field".to_string();
            tracing::error!(vm_id = %vm_id, job_id = %job_id, error = %msg);
            let _ = jobs::mark_failed(db_pool, job_id, &msg).await;
            let _ = vms::update_status(db_pool, vm_id, VmStatus::Unknown).await;
            return;
        }
    };

    // Step 1: Import (convert + push) image to local registry
    let import_result = match node_client
        .import_overlaybd_image(image_ref, &registry_url)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("Failed to import OverlayBD image: {}", e);
            tracing::error!(vm_id = %vm_id, job_id = %job_id, error = %msg);
            let _ = jobs::mark_failed(db_pool, job_id, &msg).await;
            let _ = vms::update_status(db_pool, vm_id, VmStatus::Unknown).await;
            return;
        }
    };

    let _ = jobs::update_progress(db_pool, job_id, 50).await;

    // Step 2: Persist OverlayBD disk record (boot_order = 0 marks it as the boot disk)
    let disk_id = "vda".to_string();
    let disk_record = NewVmOverlaybdDisk {
        vm_id,
        disk_id,
        image_ref: import_result.image_ref.clone(),
        image_digest: if import_result.digest.is_empty() {
            None
        } else {
            Some(import_result.digest.clone())
        },
        registry_url,
        storage_pool_id: Some(storage_pool_id),
        boot_order: 0,
    };
    if let Err(e) = vm_overlaybd_disks::create(db_pool, &disk_record).await {
        tracing::warn!(vm_id = %vm_id, error = %e, "Failed to persist OverlayBD disk record");
    }

    // Mark VM as created (node provisioning is deferred to vm start)
    let _ = vms::update_status(db_pool, vm_id, VmStatus::Created).await;
    let result = serde_json::json!({ "digest": import_result.digest });
    let _ = jobs::mark_completed(db_pool, job_id, Some(result)).await;

    tracing::info!(vm_id = %vm_id, job_id = %job_id, "VM creation job completed (overlaybd)");
}

#[utoipa::path(
    post,
    path = "/vms/{vm_id}/start",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 202, description = "VM start accepted", body = VmStartResponse),
        (status = 404, description = "VM not found"),
        (status = 422, description = "VM not in a startable state"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn start(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
) -> Result<axum::response::Response> {
    let vm = vms::get(env.pool(), vm_id).await?;
    let original_status = vm.status.clone();

    match vm.status {
        VmStatus::Running => {
            return Err(crate::errors::Error::UnprocessableEntity(
                "VM is already running".into(),
            ));
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

    // Set VM status to Pending to prevent double-start
    vms::update_status(env.pool(), vm_id, VmStatus::Pending).await?;

    // Build create request eagerly (before spawning) so we can return errors synchronously
    let create_req = if original_status == VmStatus::Created {
        Some(build_create_vm_request(&env, &vm).await.map_err(|e| {
            tracing::error!("Failed to build CreateVmRequest for {}: {}", vm_id, e);
            e
        })?)
    } else {
        None
    };

    // Create job record
    let job = jobs::create(
        env.pool(),
        NewJob {
            job_type: JobType::VmStart,
            description: Some(format!("Starting VM {}", vm.name)),
            resource_id: Some(vm_id),
            resource_type: Some("vm".to_string()),
        },
    )
    .await?;
    let job_id = job.id;

    let db_pool = env.pool_arc();

    tokio::spawn(async move {
        tracing::info!(vm_id = %vm_id, job_id = %job_id, "Starting async VM start");

        if let Err(e) = jobs::mark_running(&db_pool, job_id).await {
            tracing::error!(job_id = %job_id, error = %e, "Failed to mark job as running");
            return;
        }

        let node_client = NodeClient::new(&host.address, host.port as u16);

        // For a VM in Created state, call create_vm first
        if let Some(req) = create_req {
            if let Err(e) = node_client.create_vm(req).await {
                let msg = format!("create_vm failed: {:#}", e);
                tracing::error!(vm_id = %vm_id, job_id = %job_id, error = %msg);
                let _ = jobs::mark_failed(&db_pool, job_id, &msg).await;
                let _ = vms::update_status(&db_pool, vm_id, original_status).await;
                return;
            }
            let _ = jobs::update_progress(&db_pool, job_id, 50).await;
        }

        if let Err(e) = node_client.start_vm(vm_id).await {
            let msg = format!("start_vm failed: {:#}", e);
            tracing::error!(vm_id = %vm_id, job_id = %job_id, error = %msg);
            let _ = jobs::mark_failed(&db_pool, job_id, &msg).await;
            let _ = vms::update_status(&db_pool, vm_id, original_status).await;
            return;
        }

        let _ = vms::update_status(&db_pool, vm_id, VmStatus::Running).await;
        let _ = jobs::mark_completed(&db_pool, job_id, None).await;

        tracing::info!(vm_id = %vm_id, job_id = %job_id, "VM start job completed");
    });

    use axum::response::IntoResponse as _;
    Ok(ApiResponse {
        data: VmStartResponse { job_id },
        code: StatusCode::ACCEPTED,
    }
    .into_response())
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
    let vm = vms::get(env.pool(), vm_id).await?;

    // Only call delete_vm on the node if the VM was ever provisioned there
    if vm.status != VmStatus::Created
        && vm.status != VmStatus::Pending
        && let Ok(host) = host_for_vm(&env, vm_id).await
    {
        let node_client = NodeClient::new(&host.address, host.port as u16);
        // Tolerate node errors on delete (VM may already be gone)
        if let Err(e) = node_client.delete_vm(vm_id).await {
            tracing::warn!("delete_vm on node failed (ignoring): {}", e);
        }
    }

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

/// WebSocket console attachment for interactive terminal access
#[utoipa::path(
    get,
    path = "/vms/{vm_id}/console/attach",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    responses(
        (status = 101, description = "Switching Protocols - WebSocket connection established"),
        (status = 404, description = "VM not found"),
        (status = 422, description = "VM console not available or not in PTY mode"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env, ws))]
pub async fn console_attach(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> Result<axum::response::Response> {
    info!("WebSocket console attachment requested for VM: {}", vm_id);

    // Verify VM exists and get host
    let host = host_for_vm(&env, vm_id).await?;

    Ok(ws.on_upgrade(move |socket| handle_console_websocket(socket, vm_id, host)))
}

async fn handle_console_websocket(ws: WebSocket, vm_id: Uuid, host: crate::model::hosts::Host) {
    info!("WebSocket connection established for VM: {}", vm_id);

    let (mut ws_sender, mut ws_receiver) = ws.split();

    // Connect to qarax-node gRPC console stream
    let node_client = NodeClient::new(&host.address, host.port as u16);

    match node_client.attach_console(vm_id).await {
        Ok((grpc_input_tx, mut grpc_output_rx)) => {
            // Spawn task to forward WebSocket -> gRPC
            let ws_to_grpc = tokio::spawn(async move {
                while let Some(msg) = ws_receiver.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            if grpc_input_tx.send(text.as_bytes().to_vec()).await.is_err() {
                                break;
                            }
                        }
                        Ok(Message::Binary(data)) => {
                            if grpc_input_tx.send(data.to_vec()).await.is_err() {
                                break;
                            }
                        }
                        Ok(Message::Close(_)) => {
                            info!("WebSocket client closed connection for VM {}", vm_id);
                            break;
                        }
                        Err(e) => {
                            warn!("WebSocket error for VM {}: {}", vm_id, e);
                            break;
                        }
                        _ => {}
                    }
                }
            });

            // Spawn task to forward gRPC -> WebSocket
            let grpc_to_ws = tokio::spawn(async move {
                while let Some(result) = grpc_output_rx.recv().await {
                    match result {
                        Ok(data) => {
                            if ws_sender.send(Message::Binary(data.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(error_msg) => {
                            error!("gRPC console error for VM {}: {}", vm_id, error_msg);
                            let _ = ws_sender
                                .send(Message::Text(format!("Error: {}", error_msg).into()))
                                .await;
                            break;
                        }
                    }
                }
            });

            // Wait for either task to complete
            tokio::select! {
                _ = ws_to_grpc => {
                    info!("WebSocket to gRPC task completed for VM {}", vm_id);
                }
                _ = grpc_to_ws => {
                    info!("gRPC to WebSocket task completed for VM {}", vm_id);
                }
            }
        }
        Err(e) => {
            error!("Failed to attach to console for VM {}: {}", vm_id, e);
            let _ = ws_sender
                .send(Message::Text(
                    format!("Failed to attach to console: {}", e).into(),
                ))
                .await;
        }
    }

    info!("WebSocket console session ended for VM: {}", vm_id);
}

/// Build a `CreateVmRequest` from DB state — called at `vm start` for lazily-provisioned VMs.
async fn build_create_vm_request(env: &App, vm: &Vm) -> Result<CreateVmRequest> {
    let vm_id = vm.id;

    // Resolve boot source or fall back to defaults
    let (kernel, initramfs, default_cmdline) = if let Some(boot_source_id) = vm.boot_source_id {
        let resolved = boot_sources::resolve(env.pool(), boot_source_id)
            .await
            .map_err(|e| {
                crate::errors::Error::UnprocessableEntity(format!("Invalid boot_source_id: {}", e))
            })?;
        (
            resolved.kernel_path,
            resolved.initramfs_path,
            resolved.kernel_params,
        )
    } else {
        let d = env.vm_defaults();
        (d.kernel.clone(), d.initramfs.clone(), d.cmdline.clone())
    };

    // Load disks and filesystems
    let overlaybd_disks = vm_overlaybd_disks::list_by_vm(env.pool(), vm_id).await?;
    let filesystems = vm_filesystems::list_by_vm(env.pool(), vm_id).await?;
    let db_networks = network_interfaces::list_by_vm(env.pool(), vm_id).await?;

    let networks = net_configs_from_db(&db_networks);

    // Choose boot disk / cmdline based on what's attached
    let (overlaybd_disk, fs_configs, cmdline, memory_shared) =
        if let Some(disk) = overlaybd_disks.iter().min_by_key(|d| d.boot_order) {
            let obd = OverlaybdDiskSpec {
                disk_id: disk.disk_id.clone(),
                oci_image_ref: disk.image_ref.clone(),
                registry_url: disk.registry_url.clone(),
            };
            (
                Some(obd),
                vec![],
                "console=ttyS0 root=/dev/vda rw init=/.qarax-init".to_string(),
                false,
            )
        } else if !filesystems.is_empty() {
            let fs = &filesystems[0];
            let socket_path = format!("/var/lib/qarax/vms/{}-fs0.sock", vm_id);
            let fs_config = FsConfig {
                tag: fs.tag.clone(),
                socket: socket_path,
                num_queues: fs.num_queues,
                queue_size: fs.queue_size,
                pci_segment: fs.pci_segment,
                id: Some("fs0".to_string()),
                bootstrap_path: None,
            };
            (
                None,
                vec![fs_config],
                "console=ttyS0 root=rootfs rootfstype=virtiofs rw init=/.qarax-init".to_string(),
                true,
            )
        } else {
            // Plain boot with kernel/initramfs only
            (None, vec![], default_cmdline, vm.memory_shared)
        };

    Ok(CreateVmRequest {
        vm_id,
        boot_vcpus: vm.boot_vcpus,
        max_vcpus: vm.max_vcpus,
        memory_size: vm.memory_size,
        networks,
        kernel,
        initramfs,
        cmdline,
        fs_configs,
        memory_shared,
        overlaybd_disk,
    })
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AttachDiskRequest {
    /// Storage object ID (must be `oci_image` type).
    pub storage_object_id: Uuid,
    /// Disk identifier inside the VM (default: `"vda"`).
    pub disk_id: Option<String>,
    /// Boot priority — lower is higher priority (default: `0`).
    pub boot_order: Option<i32>,
}

/// Attach an OverlayBD storage object to a VM as a bootable disk.
#[utoipa::path(
    post,
    path = "/vms/{vm_id}/disks",
    params(
        ("vm_id" = uuid::Uuid, Path, description = "VM unique identifier")
    ),
    request_body = AttachDiskRequest,
    responses(
        (status = 201, description = "Disk attached", body = crate::model::vm_overlaybd_disks::VmOverlaybdDisk),
        (status = 404, description = "VM or storage object not found"),
        (status = 422, description = "VM not in Created state"),
        (status = 500, description = "Internal server error")
    ),
    tag = "vms"
)]
#[instrument(skip(env))]
pub async fn attach_disk(
    Extension(env): Extension<App>,
    Path(vm_id): Path<Uuid>,
    Json(req): Json<AttachDiskRequest>,
) -> Result<axum::response::Response> {
    use crate::model::vm_overlaybd_disks::VmOverlaybdDisk;
    use axum::response::IntoResponse as _;

    let vm = vms::get(env.pool(), vm_id).await?;
    if vm.status != VmStatus::Created {
        return Err(crate::errors::Error::UnprocessableEntity(
            "Disks can only be linked while the VM is in Created state".into(),
        ));
    }

    let obj = storage_objects::get(env.pool(), req.storage_object_id).await?;

    // Resolve registry URL from the object config or from the pool
    let registry_url = obj
        .config
        .get("registry_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let image_ref = obj
        .config
        .get("image_ref")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let disk_record = NewVmOverlaybdDisk {
        vm_id,
        disk_id: req.disk_id.clone().unwrap_or_else(|| "vda".to_string()),
        image_ref,
        image_digest: obj
            .config
            .get("digest")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        registry_url,
        storage_pool_id: Some(obj.storage_pool_id),
        boot_order: req.boot_order.unwrap_or(0),
    };

    let disk_id = vm_overlaybd_disks::create(env.pool(), &disk_record).await?;
    let disk = VmOverlaybdDisk {
        id: disk_id,
        vm_id: disk_record.vm_id,
        disk_id: disk_record.disk_id,
        image_ref: disk_record.image_ref,
        image_digest: disk_record.image_digest,
        registry_url: disk_record.registry_url,
        storage_pool_id: disk_record.storage_pool_id,
        boot_order: disk_record.boot_order,
    };

    Ok(ApiResponse {
        data: disk,
        code: StatusCode::CREATED,
    }
    .into_response())
}
