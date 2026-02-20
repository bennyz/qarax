use super::*;
use crate::{
    App, host_deployer,
    model::hosts::{self, DeployHostRequest, Host, HostStatus, NewHost, UpdateHostRequest},
};
use axum::{Extension, Json, extract::Path};
use http::StatusCode;
use tracing::{error, info, instrument};
use uuid::Uuid;

#[utoipa::path(
    get,
    path = "/hosts",
    responses(
        (status = 200, description = "List all hosts", body = Vec<Host>),
        (status = 500, description = "Internal server error")
    ),
    tag = "hosts"
)]
#[instrument(skip(env))]
pub async fn list(Extension(env): Extension<App>) -> Result<ApiResponse<Vec<Host>>> {
    let hosts = hosts::list(env.pool()).await?;
    Ok(ApiResponse {
        data: hosts,
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    post,
    path = "/hosts",
    request_body = NewHost,
    responses(
        (status = 201, description = "Host created successfully", body = String),
        (status = 422, description = "Invalid input"),
        (status = 409, description = "Host with name already exists"),
        (status = 500, description = "Internal server error")
    ),
    tag = "hosts"
)]
#[instrument(skip(env))]
pub async fn add(
    Extension(env): Extension<App>,
    Json(host): Json<NewHost>,
) -> Result<(StatusCode, String)> {
    host.validate_unique_name(env.pool(), &host.name).await?;
    let id = hosts::add(env.pool(), &host).await?;
    Ok((StatusCode::CREATED, id.to_string()))
}

#[utoipa::path(
    patch,
    path = "/hosts/{host_id}",
    params(
        ("host_id" = uuid::Uuid, Path, description = "Host unique identifier")
    ),
    request_body = UpdateHostRequest,
    responses(
        (status = 200, description = "Host updated successfully"),
        (status = 404, description = "Host not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "hosts"
)]
#[instrument(skip(env))]
pub async fn update(
    Extension(env): Extension<App>,
    Path(host_id): Path<Uuid>,
    Json(body): Json<UpdateHostRequest>,
) -> Result<ApiResponse<()>> {
    hosts::update_status(env.pool(), host_id, body.status).await?;
    Ok(ApiResponse {
        data: (),
        code: StatusCode::OK,
    })
}

#[utoipa::path(
    post,
    path = "/hosts/{host_id}/deploy",
    params(
        ("host_id" = uuid::Uuid, Path, description = "Host unique identifier")
    ),
    request_body = DeployHostRequest,
    responses(
        (status = 202, description = "Host deployment accepted", body = String),
        (status = 404, description = "Host not found"),
        (status = 422, description = "Invalid input"),
        (status = 500, description = "Internal server error")
    ),
    tag = "hosts"
)]
#[instrument(skip(env))]
pub async fn deploy(
    Extension(env): Extension<App>,
    Path(host_id): Path<Uuid>,
    Json(body): Json<DeployHostRequest>,
) -> Result<(StatusCode, String)> {
    body.validate()
        .map_err(crate::errors::Error::UnprocessableEntity)?;

    let host = hosts::get_by_id(env.pool(), host_id)
        .await?
        .ok_or(crate::errors::Error::NotFound)?;
    hosts::update_status(env.pool(), host_id, HostStatus::Installing).await?;

    let db_pool = env.pool_arc();
    tokio::spawn(async move {
        match host_deployer::deploy_bootc_host(&host, &body).await {
            Ok(_) => {
                info!(host_id = %host_id, "Host deployment finished successfully");
                if let Err(error) = hosts::update_status(&db_pool, host_id, HostStatus::Up).await {
                    error!(
                        host_id = %host_id,
                        error = %error,
                        "Failed to mark host status as up after deployment"
                    );
                }
            }
            Err(deploy_error) => {
                error!(
                    host_id = %host_id,
                    error = %deploy_error,
                    "Host deployment failed"
                );
                if let Err(error) =
                    hosts::update_status(&db_pool, host_id, HostStatus::InstallationFailed).await
                {
                    error!(
                        host_id = %host_id,
                        error = %error,
                        "Failed to mark host status as installation_failed"
                    );
                }
            }
        }
    });

    Ok((StatusCode::ACCEPTED, "Host deployment started".to_string()))
}
