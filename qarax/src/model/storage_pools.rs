use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Type, types::Json};
use strum_macros::{Display, EnumString};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoragePool {
    pub id: Uuid,
    pub name: String,
    pub pool_type: StoragePoolType,
    pub status: StoragePoolStatus,
    pub host_id: Option<Uuid>,
    pub config: serde_json::Value,
    pub capacity_bytes: Option<i64>,
    pub allocated_bytes: Option<i64>,
}

#[derive(sqlx::FromRow)]
pub struct StoragePoolRow {
    pub id: Uuid,
    pub name: String,
    pub pool_type: StoragePoolType,
    pub status: StoragePoolStatus,
    pub host_id: Option<Uuid>,
    pub config: Json<serde_json::Value>,
    pub capacity_bytes: Option<i64>,
    pub allocated_bytes: Option<i64>,
}

impl From<StoragePoolRow> for StoragePool {
    fn from(row: StoragePoolRow) -> Self {
        StoragePool {
            id: row.id,
            name: row.name,
            pool_type: row.pool_type,
            status: row.status,
            host_id: row.host_id,
            config: row.config.0,
            capacity_bytes: row.capacity_bytes,
            allocated_bytes: row.allocated_bytes,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq, Type, EnumString, Display)]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
#[sqlx(type_name = "storage_pool_type")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum StoragePoolType {
    Local,
    Nfs,
    Ceph,
    Lvm,
    Zfs,
}

#[derive(Deserialize, Serialize, Debug, Clone, Eq, PartialEq, Type, EnumString, Display)]
#[sqlx(rename_all = "SCREAMING_SNAKE_CASE")]
#[sqlx(type_name = "storage_pool_status")]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum StoragePoolStatus {
    Active,
    Inactive,
    Error,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewStoragePool {
    pub name: String,
    pub pool_type: StoragePoolType,
    pub host_id: Option<Uuid>,

    #[serde(default)]
    pub config: serde_json::Value,

    pub capacity_bytes: Option<i64>,
}

pub async fn list(pool: &PgPool) -> Result<Vec<StoragePool>, sqlx::Error> {
    let storage_pools: Vec<StoragePoolRow> = sqlx::query_as!(
        StoragePoolRow,
        r#"
SELECT id,
        name,
        pool_type as "pool_type: _",
        status as "status: _",
        host_id as "host_id?",
        config as "config: _",
        capacity_bytes as "capacity_bytes?",
        allocated_bytes as "allocated_bytes?"
FROM storage_pools
        "#
    )
    .fetch_all(pool)
    .await?;

    let storage_pools: Vec<StoragePool> = storage_pools
        .into_iter()
        .map(|sp: StoragePoolRow| sp.into())
        .collect();
    Ok(storage_pools)
}

pub async fn get(pool: &PgPool, pool_id: Uuid) -> Result<StoragePool, sqlx::Error> {
    let storage_pool: StoragePoolRow = sqlx::query_as!(
        StoragePoolRow,
        r#"
SELECT id,
        name,
        pool_type as "pool_type: _",
        status as "status: _",
        host_id as "host_id?",
        config as "config: _",
        capacity_bytes as "capacity_bytes?",
        allocated_bytes as "allocated_bytes?"
FROM storage_pools
WHERE id = $1
        "#,
        pool_id
    )
    .fetch_one(pool)
    .await?;

    Ok(storage_pool.into())
}
