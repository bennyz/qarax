use serde::{Deserialize, Serialize};
use sqlx::{PgPool, types::Json};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VmDisk {
    pub id: Uuid,
    pub vm_id: Uuid,
    pub storage_object_id: Uuid,
    pub device_name: String,
    pub boot_order: Option<i32>,
    pub read_only: Option<bool>,
    pub config: serde_json::Value,
}

#[derive(sqlx::FromRow)]
pub struct VmDiskRow {
    pub id: Uuid,
    pub vm_id: Uuid,
    pub storage_object_id: Uuid,
    pub device_name: String,
    pub boot_order: Option<i32>,
    pub read_only: Option<bool>,
    pub config: Json<serde_json::Value>,
}

impl From<VmDiskRow> for VmDisk {
    fn from(row: VmDiskRow) -> Self {
        VmDisk {
            id: row.id,
            vm_id: row.vm_id,
            storage_object_id: row.storage_object_id,
            device_name: row.device_name,
            boot_order: row.boot_order,
            read_only: row.read_only,
            config: row.config.0,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewVmDisk {
    pub vm_id: Uuid,
    pub storage_object_id: Uuid,
    pub device_name: String,
    pub boot_order: Option<i32>,
    pub read_only: Option<bool>,

    #[serde(default)]
    pub config: serde_json::Value,
}

pub async fn list(pool: &PgPool) -> Result<Vec<VmDisk>, sqlx::Error> {
    let vm_disks: Vec<VmDiskRow> = sqlx::query_as!(
        VmDiskRow,
        r#"
SELECT id,
        vm_id,
        storage_object_id,
        device_name,
        boot_order as "boot_order?",
        read_only as "read_only?",
        config as "config: _"
FROM vm_disks
        "#
    )
    .fetch_all(pool)
    .await?;

    let vm_disks: Vec<VmDisk> = vm_disks
        .into_iter()
        .map(|vd: VmDiskRow| vd.into())
        .collect();
    Ok(vm_disks)
}

pub async fn get(pool: &PgPool, disk_id: Uuid) -> Result<VmDisk, sqlx::Error> {
    let vm_disk: VmDiskRow = sqlx::query_as!(
        VmDiskRow,
        r#"
SELECT id,
        vm_id,
        storage_object_id,
        device_name,
        boot_order as "boot_order?",
        read_only as "read_only?",
        config as "config: _"
FROM vm_disks
WHERE id = $1
        "#,
        disk_id
    )
    .fetch_one(pool)
    .await?;

    Ok(vm_disk.into())
}

pub async fn list_by_vm(pool: &PgPool, vm_id: Uuid) -> Result<Vec<VmDisk>, sqlx::Error> {
    let vm_disks: Vec<VmDiskRow> = sqlx::query_as!(
        VmDiskRow,
        r#"
SELECT id,
        vm_id,
        storage_object_id,
        device_name,
        boot_order as "boot_order?",
        read_only as "read_only?",
        config as "config: _"
FROM vm_disks
WHERE vm_id = $1
ORDER BY boot_order NULLS LAST, device_name
        "#,
        vm_id
    )
    .fetch_all(pool)
    .await?;

    let vm_disks: Vec<VmDisk> = vm_disks
        .into_iter()
        .map(|vd: VmDiskRow| vd.into())
        .collect();
    Ok(vm_disks)
}
