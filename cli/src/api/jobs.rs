use uuid::Uuid;

use crate::client::Client;

use super::models::Job;

pub async fn get(client: &Client, job_id: Uuid) -> anyhow::Result<Job> {
    client.get(&format!("/jobs/{job_id}")).await
}
