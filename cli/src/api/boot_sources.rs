use uuid::Uuid;

use crate::client::Client;

use super::models::{BootSource, NewBootSource};

pub async fn list(client: &Client) -> anyhow::Result<Vec<BootSource>> {
    client.get("/boot-sources").await
}

pub async fn get(client: &Client, id: Uuid) -> anyhow::Result<BootSource> {
    client.get(&format!("/boot-sources/{id}")).await
}

/// Create a boot source. Returns the new boot source's UUID as plain text.
pub async fn create(client: &Client, bs: &NewBootSource) -> anyhow::Result<String> {
    client.post_text("/boot-sources", bs).await
}

pub async fn delete(client: &Client, id: Uuid) -> anyhow::Result<()> {
    client.delete(&format!("/boot-sources/{id}")).await
}
