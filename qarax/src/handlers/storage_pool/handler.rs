use super::*;
use crate::{
    App,
    grpc_client::NodeClient,
    model::{
        hosts,
        storage_pools::{self, NewStoragePool, StoragePool},
    },
};
use axum::{Extension, Json, extract::Path};
use http::StatusCode;
use serde::Deserialize;
use tracing::{instrument, warn};
use utoipa::ToSchema;
use uuid::Uuid;

#[utoipa::path(
    get,
    path = "/storage-pools",
    responses(
        (status = 200, description = "List all storage pools", body = Vec<StoragePool>),
        (status = 500, description = "Internal server error")
    ),
    tag = "storage-pools"
)]
#[instrument(skip(env))]
pub async fn list(Extension(env): Extension<App>) -> Result<ApiResponse<Vec<StoragePool>>> {
    let pools = storage_pools::list(env.pool()).await?;
    Ok(ApiResponse {
        data: pools,
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    get,
    path = "/storage-pools/{pool_id}",
    params(
        ("pool_id" = uuid::Uuid, Path, description = "Storage pool unique identifier")
    ),
    responses(
        (status = 200, description = "Storage pool found", body = StoragePool),
        (status = 404, description = "Storage pool not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "storage-pools"
)]
#[instrument(skip(env))]
pub async fn get(
    Extension(env): Extension<App>,
    Path(pool_id): Path<Uuid>,
) -> Result<ApiResponse<StoragePool>> {
    let pool = storage_pools::get(env.pool(), pool_id).await?;
    Ok(ApiResponse {
        data: pool,
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    post,
    path = "/storage-pools",
    request_body = NewStoragePool,
    responses(
        (status = 201, description = "Storage pool created successfully", body = String),
        (status = 422, description = "Invalid input"),
        (status = 500, description = "Internal server error")
    ),
    tag = "storage-pools"
)]
#[instrument(skip(env))]
pub async fn create(
    Extension(env): Extension<App>,
    Json(new_pool): Json<NewStoragePool>,
) -> Result<(StatusCode, String)> {
    let id = storage_pools::create(env.pool(), new_pool).await?;

    // Background: attach every UP host to this new pool via gRPC, then record in DB.
    let db_pool = env.pool_arc();
    tokio::spawn(async move {
        let pool = match storage_pools::get(&db_pool, id).await {
            Ok(p) => p,
            Err(e) => {
                warn!(pool_id = %id, error = %e, "Failed to fetch new pool for host attachment");
                return;
            }
        };

        let up_hosts = match hosts::list_up(&db_pool).await {
            Ok(h) => h,
            Err(e) => {
                warn!(pool_id = %id, error = %e, "Failed to list UP hosts for pool attachment");
                return;
            }
        };

        for host in up_hosts {
            let client = NodeClient::new(&host.address, host.port as u16);
            match client.attach_storage_pool(&pool).await {
                Ok(()) => {
                    if let Err(e) = storage_pools::attach_host(&db_pool, id, host.id).await {
                        warn!(pool_id = %id, host_id = %host.id, error = %e, "Failed to record pool attachment in DB");
                    }
                }
                Err(e) => {
                    warn!(pool_id = %id, host_id = %host.id, error = %e, "Failed to attach storage pool to host via gRPC");
                }
            }
        }
    });

    Ok((StatusCode::CREATED, id.to_string()))
}

#[utoipa::path(
    delete,
    path = "/storage-pools/{pool_id}",
    params(
        ("pool_id" = uuid::Uuid, Path, description = "Storage pool unique identifier")
    ),
    responses(
        (status = 204, description = "Storage pool deleted successfully"),
        (status = 404, description = "Storage pool not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "storage-pools"
)]
#[instrument(skip(env))]
pub async fn delete(
    Extension(env): Extension<App>,
    Path(pool_id): Path<Uuid>,
) -> Result<StatusCode> {
    storage_pools::delete(env.pool(), pool_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AttachHostRequest {
    pub host_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/storage-pools/{pool_id}/hosts",
    params(
        ("pool_id" = uuid::Uuid, Path, description = "Storage pool unique identifier")
    ),
    request_body = AttachHostRequest,
    responses(
        (status = 204, description = "Host attached to storage pool"),
        (status = 404, description = "Storage pool or host not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "storage-pools"
)]
#[instrument(skip(env))]
pub async fn attach_host(
    Extension(env): Extension<App>,
    Path(pool_id): Path<Uuid>,
    Json(body): Json<AttachHostRequest>,
) -> Result<StatusCode> {
    let pool = storage_pools::get(env.pool(), pool_id).await?;
    let host = hosts::get_by_id(env.pool(), body.host_id)
        .await?
        .ok_or(crate::errors::Error::NotFound)?;

    // Perform the real attachment on the node first.
    let client = NodeClient::new(&host.address, host.port as u16);
    client.attach_storage_pool(&pool).await.map_err(|e| {
        tracing::error!(
            pool_id = %pool_id,
            host_id = %body.host_id,
            error = %e,
            "gRPC attach_storage_pool failed"
        );
        crate::errors::Error::InternalServerError
    })?;

    // Record the attachment in the DB.
    storage_pools::attach_host(env.pool(), pool_id, body.host_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/storage-pools/{pool_id}/hosts/{host_id}",
    params(
        ("pool_id" = uuid::Uuid, Path, description = "Storage pool unique identifier"),
        ("host_id" = uuid::Uuid, Path, description = "Host unique identifier")
    ),
    responses(
        (status = 204, description = "Host detached from storage pool"),
        (status = 404, description = "Host not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "storage-pools"
)]
#[instrument(skip(env))]
pub async fn detach_host(
    Extension(env): Extension<App>,
    Path((pool_id, host_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let pool = storage_pools::get(env.pool(), pool_id).await?;
    let host = hosts::get_by_id(env.pool(), host_id)
        .await?
        .ok_or(crate::errors::Error::NotFound)?;

    // Perform the real detachment on the node first.
    let client = NodeClient::new(&host.address, host.port as u16);
    client.detach_storage_pool(&pool).await.map_err(|e| {
        tracing::error!(
            pool_id = %pool_id,
            host_id = %host_id,
            error = %e,
            "gRPC detach_storage_pool failed"
        );
        crate::errors::Error::InternalServerError
    })?;

    // Remove the DB record.
    storage_pools::detach_host(env.pool(), pool_id, host_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
