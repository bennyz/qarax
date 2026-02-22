use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row, Type, types::Uuid};
use strum_macros::{Display, EnumString};
use utoipa::ToSchema;
use validator::{Validate, ValidationError, ValidationErrors};

use crate::errors;

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Host {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub port: i32,
    pub status: HostStatus,
    pub host_user: String,

    #[serde(skip_deserializing)]
    pub password: Vec<u8>,

    pub cloud_hypervisor_version: Option<String>,
    pub kernel_version: Option<String>,
}

#[derive(
    Deserialize, Serialize, Debug, Clone, Eq, PartialEq, Type, EnumString, Display, ToSchema,
)]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
#[sqlx(type_name = "host_status")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum HostStatus {
    Unknown,
    Down,
    Installing,
    InstallationFailed,
    Initializing,
    Up,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct UpdateHostRequest {
    pub status: HostStatus,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct DeployHostRequest {
    /// Fully-qualified bootc image reference to deploy on the host.
    pub image: String,
    /// SSH port used to reach the host. Defaults to 22.
    pub ssh_port: Option<u16>,
    /// SSH user override. Defaults to the host's registered `host_user`.
    pub ssh_user: Option<String>,
    /// Optional SSH password override for this deployment request.
    pub ssh_password: Option<String>,
    /// Optional SSH private key path on the qarax control-plane host.
    pub ssh_private_key_path: Option<String>,
    /// Install bootc before switching image. Defaults to true.
    pub install_bootc: Option<bool>,
    /// Reboot after `bootc switch`. Defaults to true.
    pub reboot: Option<bool>,
}

impl DeployHostRequest {
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.image.trim().is_empty() {
            return Err("image is required".to_string());
        }

        if self.ssh_password.is_some() && self.ssh_private_key_path.is_some() {
            return Err(
                "provide either ssh_password or ssh_private_key_path, but not both".to_string(),
            );
        }

        if let Some(user) = &self.ssh_user
            && user.trim().is_empty()
        {
            return Err("ssh_user cannot be empty".to_string());
        }

        if let Some(path) = &self.ssh_private_key_path
            && path.trim().is_empty()
        {
            return Err("ssh_private_key_path cannot be empty".to_string());
        }

        if let Some(port) = self.ssh_port
            && port == 0
        {
            return Err("ssh_port must be greater than 0".to_string());
        }

        Ok(())
    }

    pub fn ssh_port(&self) -> u16 {
        self.ssh_port.unwrap_or(22)
    }

    pub fn install_bootc(&self) -> bool {
        self.install_bootc.unwrap_or(true)
    }

    pub fn reboot(&self) -> bool {
        self.reboot.unwrap_or(true)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Validate, ToSchema)]
pub struct NewHost {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub address: String,

    #[validate(range(min = 1, max = 65535))]
    pub port: i32,

    pub host_user: String,
    pub password: String,
}

impl NewHost {
    pub async fn validate_unique_name(
        &self,
        pool: &PgPool,
        name: &str,
    ) -> Result<(), errors::Error> {
        let host = by_name(pool, name).await.map_err(errors::Error::Sqlx)?;

        if host.is_some() {
            let mut errors = ValidationErrors::new();
            errors.add("name", ValidationError::new("unique_name"));
            return Err(errors::Error::InvalidEntity(errors));
        }

        Ok(())
    }
}

pub async fn list(pool: &PgPool) -> Result<Vec<Host>, sqlx::Error> {
    let hosts = sqlx::query_as!(
        Host,
        r#"
        SELECT id, name, address, port, host_user, password,
               status as "status: _",
               cloud_hypervisor_version, kernel_version
        FROM hosts
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(hosts)
}

// add adds a new host and returns its generated id
pub async fn add(pool: &PgPool, host: &NewHost) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query(
        r#"
        INSERT INTO hosts (name, address, port, host_user, password, status)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#,
    )
    .bind(&host.name)
    .bind(&host.address)
    .bind(host.port)
    .bind(&host.host_user)
    .bind(host.password.as_bytes())
    .bind(HostStatus::Down)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("Error adding host: {}", e);
        e
    })?;

    Ok(row.get("id"))
}

pub async fn update_status(pool: &PgPool, id: Uuid, status: HostStatus) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE hosts SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns a host by id, if it exists.
pub async fn get_by_id(pool: &PgPool, host_id: Uuid) -> Result<Option<Host>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, name, address, port, host_user, password, status, cloud_hypervisor_version, kernel_version FROM hosts WHERE id = $1",
    )
    .bind(host_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Host {
        id: r.get("id"),
        name: r.get("name"),
        address: r.get("address"),
        port: r.get("port"),
        host_user: r.get("host_user"),
        password: r.get("password"),
        status: r.get("status"),
        cloud_hypervisor_version: r.get("cloud_hypervisor_version"),
        kernel_version: r.get("kernel_version"),
    }))
}

/// Pick a random UP host for VM scheduling.
pub async fn pick_up_host(pool: &PgPool) -> Result<Option<Host>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, name, address, port, host_user, password, status, cloud_hypervisor_version, kernel_version FROM hosts WHERE status = 'UP' ORDER BY RANDOM() LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Host {
        id: r.get("id"),
        name: r.get("name"),
        address: r.get("address"),
        port: r.get("port"),
        host_user: r.get("host_user"),
        password: r.get("password"),
        status: r.get("status"),
        cloud_hypervisor_version: r.get("cloud_hypervisor_version"),
        kernel_version: r.get("kernel_version"),
    }))
}

/// Update version information for a host (called after GetNodeInfo).
pub async fn update_versions(
    pool: &PgPool,
    id: Uuid,
    ch_version: &str,
    kernel_version: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hosts SET cloud_hypervisor_version = $1, kernel_version = $2 WHERE id = $3",
    )
    .bind(ch_version)
    .bind(kernel_version)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// TODO: figure out how to not fetch the entire host. Maybe with SELECT exists()?
pub async fn by_name(pool: &PgPool, name: &str) -> Result<Option<Host>, sqlx::Error> {
    let host = sqlx::query_as!(
        Host,
        r#"
        SELECT id, name, address, port, host_user, password,
               status as "status: _",
               cloud_hypervisor_version, kernel_version
        FROM hosts
        WHERE name = $1
        "#,
        name,
    )
    .fetch_optional(pool)
    .await?;

    Ok(host)
}

#[cfg(test)]
mod tests {
    use super::DeployHostRequest;

    #[test]
    fn deploy_request_rejects_empty_image() {
        let request = DeployHostRequest {
            image: "   ".to_string(),
            ssh_port: None,
            ssh_user: None,
            ssh_password: None,
            ssh_private_key_path: None,
            install_bootc: None,
            reboot: None,
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn deploy_request_rejects_multiple_auth_methods() {
        let request = DeployHostRequest {
            image: "quay.io/example/qarax-vmm:v1".to_string(),
            ssh_port: Some(22),
            ssh_user: Some("root".to_string()),
            ssh_password: Some("secret".to_string()),
            ssh_private_key_path: Some("/tmp/id_rsa".to_string()),
            install_bootc: Some(true),
            reboot: Some(true),
        };

        assert!(request.validate().is_err());
    }
}
