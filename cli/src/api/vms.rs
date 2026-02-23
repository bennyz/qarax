use anyhow::{Context, anyhow};
use reqwest::StatusCode;
use uuid::Uuid;

use crate::client::Client;

use super::models::{CreateVmResponse, CreateVmResult, NewVm, Vm};

pub async fn list(client: &Client) -> anyhow::Result<Vec<Vm>> {
    client.get("/vms").await
}

pub async fn get(client: &Client, vm_id: Uuid) -> anyhow::Result<Vm> {
    client.get(&format!("/vms/{vm_id}")).await
}

/// Creates a VM. The server returns 201 (sync, body is a JSON-quoted UUID string)
/// or 202 (async with image pull, body is `CreateVmResponse`).
pub async fn create(client: &Client, vm: &NewVm) -> anyhow::Result<CreateVmResult> {
    let resp = client.post_response("/vms", vm).await?;
    match resp.status() {
        StatusCode::CREATED => {
            let id_str: String = resp.json().await.context("failed to parse VM id")?;
            let vm_id =
                Uuid::parse_str(&id_str).with_context(|| format!("invalid UUID: {id_str}"))?;
            Ok(CreateVmResult::Created(vm_id))
        }
        StatusCode::ACCEPTED => {
            let body: CreateVmResponse = resp
                .json()
                .await
                .context("failed to parse CreateVmResponse")?;
            Ok(CreateVmResult::Accepted {
                vm_id: body.vm_id,
                job_id: body.job_id,
            })
        }
        status => Err(anyhow!("unexpected status: {status}")),
    }
}

pub async fn delete(client: &Client, vm_id: Uuid) -> anyhow::Result<()> {
    client.delete(&format!("/vms/{vm_id}")).await
}

pub async fn start(client: &Client, vm_id: Uuid) -> anyhow::Result<()> {
    client.post_empty(&format!("/vms/{vm_id}/start")).await
}

pub async fn stop(client: &Client, vm_id: Uuid) -> anyhow::Result<()> {
    client.post_empty(&format!("/vms/{vm_id}/stop")).await
}

pub async fn pause(client: &Client, vm_id: Uuid) -> anyhow::Result<()> {
    client.post_empty(&format!("/vms/{vm_id}/pause")).await
}

pub async fn resume(client: &Client, vm_id: Uuid) -> anyhow::Result<()> {
    client.post_empty(&format!("/vms/{vm_id}/resume")).await
}

pub async fn console_log(client: &Client, vm_id: Uuid) -> anyhow::Result<String> {
    client.get_text(&format!("/vms/{vm_id}/console")).await
}
