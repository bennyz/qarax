use uuid::Uuid;

use crate::client::Client;

use super::models::{AttachHostToNetworkRequest, IpAllocation, Network, NewNetwork};

pub async fn list(client: &Client) -> anyhow::Result<Vec<Network>> {
    client.get("/networks").await
}

pub async fn get(client: &Client, id: Uuid) -> anyhow::Result<Network> {
    client.get(&format!("/networks/{id}")).await
}

pub async fn create(client: &Client, network: &NewNetwork) -> anyhow::Result<String> {
    client.post_text("/networks", network).await
}

pub async fn delete(client: &Client, id: Uuid) -> anyhow::Result<()> {
    client.delete(&format!("/networks/{id}")).await
}

pub async fn attach_host(
    client: &Client,
    network_id: Uuid,
    host_id: Uuid,
    bridge_name: &str,
) -> anyhow::Result<()> {
    client
        .post_response(
            &format!("/networks/{network_id}/hosts"),
            &AttachHostToNetworkRequest {
                host_id,
                bridge_name: bridge_name.to_string(),
            },
        )
        .await?;
    Ok(())
}

pub async fn detach_host(
    client: &Client,
    network_id: Uuid,
    host_id: Uuid,
) -> anyhow::Result<()> {
    client
        .delete(&format!("/networks/{network_id}/hosts/{host_id}"))
        .await
}

pub async fn list_ips(client: &Client, network_id: Uuid) -> anyhow::Result<Vec<IpAllocation>> {
    client
        .get(&format!("/networks/{network_id}/ips"))
        .await
}
