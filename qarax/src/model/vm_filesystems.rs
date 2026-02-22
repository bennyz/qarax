use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct VmFilesystem {
    pub id: Uuid,
    pub vm_id: Uuid,
    pub tag: String,
    pub num_queues: i32,
    pub queue_size: i32,
    pub pci_segment: Option<i32>,
    pub image_ref: Option<String>,
    pub image_digest: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct VmFilesystemRow {
    pub id: Uuid,
    pub vm_id: Uuid,
    pub tag: String,
    pub num_queues: i32,
    pub queue_size: i32,
    pub pci_segment: Option<i32>,
    pub image_ref: Option<String>,
    pub image_digest: Option<String>,
}

impl From<VmFilesystemRow> for VmFilesystem {
    fn from(row: VmFilesystemRow) -> Self {
        VmFilesystem {
            id: row.id,
            vm_id: row.vm_id,
            tag: row.tag,
            num_queues: row.num_queues,
            queue_size: row.queue_size,
            pci_segment: row.pci_segment,
            image_ref: row.image_ref,
            image_digest: row.image_digest,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct NewVmFilesystem {
    pub vm_id: Uuid,
    pub tag: String,
    pub num_queues: Option<i32>,
    pub queue_size: Option<i32>,
    pub pci_segment: Option<i32>,
    pub image_ref: Option<String>,
    pub image_digest: Option<String>,
}

pub async fn list_by_vm(pool: &PgPool, vm_id: Uuid) -> Result<Vec<VmFilesystem>, sqlx::Error> {
    let rows: Vec<VmFilesystemRow> = sqlx::query_as!(
        VmFilesystemRow,
        r#"
SELECT id,
       vm_id,
       tag,
       num_queues as "num_queues!",
       queue_size as "queue_size!",
       pci_segment as "pci_segment?",
       image_ref as "image_ref?",
       image_digest as "image_digest?"
FROM vm_filesystems
WHERE vm_id = $1
ORDER BY tag
        "#,
        vm_id
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn create(pool: &PgPool, fs: &NewVmFilesystem) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query(
        r#"
INSERT INTO vm_filesystems (
    id, vm_id, tag, num_queues, queue_size, pci_segment, image_ref, image_digest
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(id)
    .bind(fs.vm_id)
    .bind(&fs.tag)
    .bind(fs.num_queues.unwrap_or(1))
    .bind(fs.queue_size.unwrap_or(1024))
    .bind(fs.pci_segment)
    .bind(&fs.image_ref)
    .bind(&fs.image_digest)
    .execute(pool)
    .await?;

    Ok(id)
}

pub async fn create_tx(
    tx: &mut Transaction<'_, Postgres>,
    fs: &NewVmFilesystem,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();

    sqlx::query!(
        r#"
INSERT INTO vm_filesystems (
    id, vm_id, tag, num_queues, queue_size, pci_segment, image_ref, image_digest
)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        id,
        fs.vm_id,
        fs.tag,
        fs.num_queues.unwrap_or(1),
        fs.queue_size.unwrap_or(1024),
        fs.pci_segment,
        fs.image_ref,
        fs.image_digest,
    )
    .execute(tx.as_mut())
    .await?;

    Ok(id)
}

pub async fn delete_by_vm(pool: &PgPool, vm_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM vm_filesystems WHERE vm_id = $1", vm_id)
        .execute(pool)
        .await?;
    Ok(())
}
