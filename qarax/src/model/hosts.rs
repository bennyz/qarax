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
        SELECT id, name, address, port, host_user, password, status as "status: _"
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
        "SELECT id, name, address, port, host_user, password, status FROM hosts WHERE id = $1",
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
    }))
}

/// Pick a random UP host for VM scheduling.
pub async fn pick_up_host(pool: &PgPool) -> Result<Option<Host>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, name, address, port, host_user, password, status FROM hosts WHERE status = 'UP' ORDER BY RANDOM() LIMIT 1",
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
    }))
}

// TODO: figure out how to not fetch the entire host. Maybe with SELECT exists()?
pub async fn by_name(pool: &PgPool, name: &str) -> Result<Option<Host>, sqlx::Error> {
    let host = sqlx::query_as!(
        Host,
        r#"
        SELECT id, name, address, port, host_user, password, status as "status: _"
        FROM hosts
        WHERE name = $1
        "#,
        name,
    )
    .fetch_optional(pool)
    .await?;

    Ok(host)
}
