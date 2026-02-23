use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct VmOverlaybdDisk {
    pub id: Uuid,
    pub vm_id: Uuid,
    pub disk_id: String,
    pub image_ref: String,
    pub image_digest: Option<String>,
    pub registry_url: String,
    pub storage_pool_id: Option<Uuid>,
    pub boot_order: i32,
}

#[derive(sqlx::FromRow)]
pub struct VmOverlaybdDiskRow {
    pub id: Uuid,
    pub vm_id: Uuid,
    pub disk_id: String,
    pub image_ref: String,
    pub image_digest: Option<String>,
    pub registry_url: String,
    pub storage_pool_id: Option<Uuid>,
    pub boot_order: i32,
}

impl From<VmOverlaybdDiskRow> for VmOverlaybdDisk {
    fn from(row: VmOverlaybdDiskRow) -> Self {
        VmOverlaybdDisk {
            id: row.id,
            vm_id: row.vm_id,
            disk_id: row.disk_id,
            image_ref: row.image_ref,
            image_digest: row.image_digest,
            registry_url: row.registry_url,
            storage_pool_id: row.storage_pool_id,
            boot_order: row.boot_order,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct NewVmOverlaybdDisk {
    pub vm_id: Uuid,
    pub disk_id: String,
    pub image_ref: String,
    pub image_digest: Option<String>,
    pub registry_url: String,
    pub storage_pool_id: Option<Uuid>,
    pub boot_order: i32,
}

pub async fn create(pool: &PgPool, disk: &NewVmOverlaybdDisk) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        r#"
INSERT INTO vm_overlaybd_disks (
    id, vm_id, disk_id, image_ref, image_digest, registry_url, storage_pool_id, boot_order
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(id)
    .bind(disk.vm_id)
    .bind(&disk.disk_id)
    .bind(&disk.image_ref)
    .bind(&disk.image_digest)
    .bind(&disk.registry_url)
    .bind(disk.storage_pool_id)
    .bind(disk.boot_order)
    .execute(pool)
    .await?;

    Ok(id)
}

pub async fn list_by_vm(pool: &PgPool, vm_id: Uuid) -> Result<Vec<VmOverlaybdDisk>, sqlx::Error> {
    let rows: Vec<VmOverlaybdDiskRow> = sqlx::query_as::<_, VmOverlaybdDiskRow>(
        r#"
SELECT id,
       vm_id,
       disk_id,
       image_ref,
       image_digest,
       registry_url,
       storage_pool_id,
       boot_order
FROM vm_overlaybd_disks
WHERE vm_id = $1
ORDER BY boot_order, disk_id
        "#,
    )
    .bind(vm_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn delete_by_vm(pool: &PgPool, vm_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM vm_overlaybd_disks WHERE vm_id = $1")
        .bind(vm_id)
        .execute(pool)
        .await?;
    Ok(())
}
