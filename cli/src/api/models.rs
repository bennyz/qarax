use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── VMs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Vm {
    pub id: Uuid,
    pub name: String,
    pub host_id: Option<Uuid>,
    pub status: String,
    pub hypervisor: String,
    pub boot_source_id: Option<Uuid>,
    pub description: Option<String>,
    pub boot_vcpus: i32,
    pub max_vcpus: i32,
    pub memory_size: i64,
    pub image_ref: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NewVm {
    pub name: String,
    pub hypervisor: String,
    pub boot_vcpus: i32,
    pub max_vcpus: i32,
    pub memory_size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_source_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_ref: Option<String>,
    pub config: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateVmResponse {
    pub vm_id: Uuid,
    pub job_id: Uuid,
}

pub enum CreateVmResult {
    Created(Uuid),
    Accepted { vm_id: Uuid, job_id: Uuid },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VmStartResponse {
    pub job_id: Uuid,
}

// ─── Hosts ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Host {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub port: i32,
    pub status: String,
    pub host_user: String,
    pub cloud_hypervisor_version: Option<String>,
    pub kernel_version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NewHost {
    pub name: String,
    pub address: String,
    pub port: i32,
    pub host_user: String,
    pub password: String,
}

#[derive(Debug, Serialize, Default)]
pub struct DeployHostRequest {
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_private_key_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_bootc: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reboot: Option<bool>,
}

// ─── Storage ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct StoragePool {
    pub id: Uuid,
    pub name: String,
    pub pool_type: String,
    pub status: String,
    pub capacity_bytes: Option<i64>,
    pub allocated_bytes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct NewStoragePool {
    pub name: String,
    pub pool_type: String,
    pub config: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_bytes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AttachHostToPoolRequest {
    pub host_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageObject {
    pub id: Uuid,
    pub name: String,
    pub storage_pool_id: Uuid,
    pub object_type: String,
    pub size_bytes: i64,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct NewStorageObject {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_pool_id: Option<Uuid>,
    pub object_type: String,
    pub size_bytes: i64,
    pub config: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
}

// ─── Transfers ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Transfer {
    pub id: Uuid,
    pub name: String,
    pub transfer_type: String,
    pub status: String,
    pub source: String,
    pub storage_pool_id: Uuid,
    pub object_type: String,
    pub storage_object_id: Option<Uuid>,
    pub total_bytes: Option<i64>,
    pub transferred_bytes: i64,
    pub error_message: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NewTransfer {
    pub name: String,
    pub source: String,
    pub object_type: String,
}

// ─── Boot sources ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct BootSource {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub kernel_image_id: Uuid,
    pub kernel_params: Option<String>,
    pub initrd_image_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct NewBootSource {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub kernel_image_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kernel_params: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initrd_image_id: Option<Uuid>,
}

// ─── Jobs ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub job_type: String,
    pub status: String,
    pub description: Option<String>,
    pub resource_id: Option<Uuid>,
    pub progress: Option<i32>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ─── VM disks ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AttachDiskRequest {
    pub storage_object_id: Uuid,
    pub disk_id: Option<String>,
    pub boot_order: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
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

// ─── Storage pool import ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportToPoolRequest {
    pub name: String,
    pub image_ref: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportToPoolResponse {
    pub job_id: Uuid,
    pub storage_object_id: Uuid,
}
