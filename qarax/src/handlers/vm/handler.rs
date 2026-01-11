use axum::Extension;
use http::StatusCode;
use tracing::instrument;

use crate::{
    App,
    model::vms::{self, Vm},
};

use super::{ApiResponse, Result};

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

#[allow(dead_code)]
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
pub async fn get(Extension(env): Extension<App>, vm_id: uuid::Uuid) -> Result<ApiResponse<Vm>> {
    let vm = vms::get(env.pool(), vm_id).await?;
    Ok(ApiResponse {
        data: vm,
        code: StatusCode::OK,
    })
}
