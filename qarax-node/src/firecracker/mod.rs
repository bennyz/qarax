//! Firecracker VMM backend.
//!
//! Implements `VmmManager` for Firecracker microVMs using the Firecracker REST
//! API over a Unix socket (identical transport to Cloud Hypervisor, different
//! endpoint paths).
//!
//! Lifecycle mapping:
//!   create_vm  — spawn process + configure machine/boot/drives/nets
//!   start_vm   — PUT /actions {"action_type":"InstanceStart"}
//!   stop_vm    — PUT /actions {"action_type":"SendCtrlAltDel"}
//!   force_stop — kill the process
//!   pause_vm   — PATCH /vm {"state":"Paused"}
//!   resume_vm  — PATCH /vm {"state":"Resumed"}
//!   delete_vm  — kill + cleanup
//!   snapshot_vm — pause + PUT /snapshot/create
//!   restore_vm  — spawn + PUT /snapshot/load

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use futures::StreamExt;
use http_body_util::{Empty, Full, combinators::BoxBody};
use hyper::Request;
use prost::Message;
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::rpc::node::{VmConfig as ProtoVmConfig, VmState, VmStatus};
use crate::vmm::{VmmError, VmmManager};
use cloud_hypervisor_sdk::client::TokioIo;

struct FcVmInstance {
    proto_config: ProtoVmConfig,
    process: Option<Child>,
    socket_path: PathBuf,
    status: VmStatus,
    tap_devices: Vec<String>,
}

impl FcVmInstance {
    fn to_vm_state(&self) -> VmState {
        VmState {
            config: Some(self.proto_config.clone()),
            status: self.status.into(),
            memory_actual_size: None,
        }
    }
}

pub struct FirecrackerManager {
    runtime_dir: PathBuf,
    fc_binary: PathBuf,
    vms: Arc<Mutex<HashMap<String, FcVmInstance>>>,
}

impl FirecrackerManager {
    pub fn new(runtime_dir: impl Into<PathBuf>, fc_binary: impl Into<PathBuf>) -> Self {
        let runtime_dir = runtime_dir.into();
        let fc_binary = fc_binary.into();
        info!(
            "FirecrackerManager initialized: runtime_dir={}, fc_binary={}",
            runtime_dir.display(),
            fc_binary.display()
        );
        Self {
            runtime_dir,
            fc_binary,
            vms: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

mod helpers;

#[async_trait::async_trait]
impl VmmManager for FirecrackerManager {
    async fn create_vm(&self, config: ProtoVmConfig) -> Result<VmState, VmmError> {
        let vm_id = config.vm_id.clone();
        info!("FC: Creating VM {}", vm_id);

        {
            let vms = self.vms.lock().await;
            if vms.contains_key(&vm_id) {
                return Err(VmmError::VmAlreadyExists(vm_id));
            }
        }

        tokio::fs::create_dir_all(&self.runtime_dir)
            .await
            .map_err(VmmError::SpawnError)?;

        let mut config = config;

        // Create TAP devices for network interfaces that don't have one yet.
        let mut tap_devices: Vec<String> = Vec::new();
        for (i, net) in config.networks.iter_mut().enumerate() {
            if net.tap.is_none() && !net.vhost_user.unwrap_or(false) {
                let tap_name = Self::tap_name_for_net(&vm_id, i);
                if let Err(e) = Self::create_tap_device(&tap_name).await {
                    for tap in &tap_devices {
                        Self::delete_tap_device(tap).await;
                    }
                    return Err(e);
                }
                net.tap = Some(tap_name.clone());
                tap_devices.push(tap_name);
            }
        }

        // Attach TAPs to bridges if specified.
        for net in &config.networks {
            if let (Some(tap_name), Some(bridge_name)) = (&net.tap, &net.bridge)
                && let Err(e) =
                    crate::networking::bridge::attach_to_bridge(tap_name, bridge_name).await
            {
                for tap in &tap_devices {
                    Self::delete_tap_device(tap).await;
                }
                return Err(VmmError::TapError(format!(
                    "Failed to attach TAP {} to bridge {}: {}",
                    tap_name, bridge_name, e
                )));
            }
        }

        // Build cloud-init seed image if configured.
        if let Some(ci) = &config.cloud_init
            && !ci.user_data.is_empty()
        {
            let seed_path = self.cloud_init_seed_path(&vm_id);
            let network_config =
                (!ci.network_config.is_empty()).then_some(ci.network_config.as_str());
            let buf =
                crate::cloud_init::build_seed_image(&ci.user_data, &ci.meta_data, network_config)
                    .map_err(|e| VmmError::InvalidConfig(e.to_string()))?;
            tokio::fs::write(&seed_path, buf)
                .await
                .map_err(VmmError::SpawnError)?;
            config.disks.push(crate::rpc::node::DiskConfig {
                id: "cidata".to_string(),
                path: Some(seed_path.display().to_string()),
                readonly: Some(true),
                ..Default::default()
            });
            info!("FC: cloud-init seed disk attached for VM {}", vm_id);
        }

        let socket_path = self.socket_path(&vm_id);
        let log_path = self.log_path(&vm_id);
        let config_path = self.config_path(&vm_id);

        if socket_path.exists() {
            let _ = tokio::fs::remove_file(&socket_path).await;
        }

        let log_file = tokio::fs::File::create(&log_path)
            .await
            .map_err(VmmError::SpawnError)?
            .into_std()
            .await;
        let stderr_file = log_file.try_clone().map_err(VmmError::SpawnError)?;

        let process = match Command::new(&self.fc_binary)
            .arg("--api-sock")
            .arg(&socket_path)
            .stdout(std::process::Stdio::from(log_file))
            .stderr(std::process::Stdio::from(stderr_file))
            .kill_on_drop(true)
            .spawn()
        {
            Ok(p) => p,
            Err(e) => {
                for tap in &tap_devices {
                    Self::delete_tap_device(tap).await;
                }
                return Err(VmmError::SpawnError(e));
            }
        };

        info!("FC: process started with PID {:?}", process.id());

        if let Err(e) = Self::wait_for_socket(&socket_path).await {
            for tap in &tap_devices {
                Self::delete_tap_device(tap).await;
            }
            return Err(e);
        }

        // Configure machine, boot source, drives, networks.
        if let Err(e) = Self::configure_vm(&socket_path, &config).await {
            for tap in &tap_devices {
                Self::delete_tap_device(tap).await;
            }
            return Err(e);
        }

        // Persist config for recovery.
        let config_bytes = config.encode_to_vec();
        if let Err(e) = tokio::fs::write(&config_path, config_bytes).await {
            warn!("FC: Failed to persist config for VM {}: {}", vm_id, e);
        }

        let instance = FcVmInstance {
            proto_config: config.clone(),
            process: Some(process),
            socket_path,
            status: VmStatus::Created,
            tap_devices,
        };

        let state = instance.to_vm_state();
        {
            let mut vms = self.vms.lock().await;
            vms.insert(vm_id.clone(), instance);
        }

        info!("FC: VM {} created successfully", vm_id);
        Ok(state)
    }

    async fn start_vm(&self, vm_id: &str) -> Result<(), VmmError> {
        info!("FC: Starting VM {}", vm_id);

        let socket_path = {
            let vms = self.vms.lock().await;
            let instance = vms
                .get(vm_id)
                .ok_or_else(|| VmmError::VmNotFound(vm_id.to_string()))?;
            instance.socket_path.clone()
        };

        let body = r#"{"action_type":"InstanceStart"}"#;
        Self::fc_api(&socket_path, "PUT", "/actions", Some(body)).await?;

        {
            let mut vms = self.vms.lock().await;
            if let Some(instance) = vms.get_mut(vm_id) {
                instance.status = VmStatus::Running;
            }
        }

        info!("FC: VM {} started successfully", vm_id);
        Ok(())
    }

    async fn stop_vm(&self, vm_id: &str) -> Result<(), VmmError> {
        info!("FC: Stopping VM {}", vm_id);

        let socket_path = {
            let vms = self.vms.lock().await;
            let instance = vms
                .get(vm_id)
                .ok_or_else(|| VmmError::VmNotFound(vm_id.to_string()))?;
            instance.socket_path.clone()
        };

        let body = r#"{"action_type":"SendCtrlAltDel"}"#;
        match Self::fc_api(&socket_path, "PUT", "/actions", Some(body)).await {
            Ok(_) => {}
            Err(e) => {
                warn!(
                    "FC: VM {} soft-stop failed (treating as stopped): {}",
                    vm_id, e
                );
            }
        }

        {
            let mut vms = self.vms.lock().await;
            if let Some(instance) = vms.get_mut(vm_id) {
                instance.status = VmStatus::Shutdown;
            }
        }

        info!("FC: VM {} stopped", vm_id);
        Ok(())
    }

    async fn force_stop_vm(&self, vm_id: &str) -> Result<(), VmmError> {
        info!("FC: Force stopping VM {}", vm_id);

        let mut vms = self.vms.lock().await;
        let instance = vms
            .get_mut(vm_id)
            .ok_or_else(|| VmmError::VmNotFound(vm_id.to_string()))?;

        if let Some(mut process) = instance.process.take()
            && let Err(e) = process.kill().await
        {
            warn!("FC: Failed to kill process for VM {}: {}", vm_id, e);
        }

        instance.status = VmStatus::Shutdown;
        info!("FC: VM {} force stopped", vm_id);
        Ok(())
    }

    async fn pause_vm(&self, vm_id: &str) -> Result<(), VmmError> {
        info!("FC: Pausing VM {}", vm_id);

        let socket_path = {
            let vms = self.vms.lock().await;
            let instance = vms
                .get(vm_id)
                .ok_or_else(|| VmmError::VmNotFound(vm_id.to_string()))?;
            instance.socket_path.clone()
        };

        let body = r#"{"state":"Paused"}"#;
        Self::fc_api(&socket_path, "PATCH", "/vm", Some(body)).await?;

        {
            let mut vms = self.vms.lock().await;
            if let Some(instance) = vms.get_mut(vm_id) {
                instance.status = VmStatus::Paused;
            }
        }

        info!("FC: VM {} paused", vm_id);
        Ok(())
    }

    async fn resume_vm(&self, vm_id: &str) -> Result<(), VmmError> {
        info!("FC: Resuming VM {}", vm_id);

        let socket_path = {
            let vms = self.vms.lock().await;
            let instance = vms
                .get(vm_id)
                .ok_or_else(|| VmmError::VmNotFound(vm_id.to_string()))?;
            instance.socket_path.clone()
        };

        let body = r#"{"state":"Resumed"}"#;
        Self::fc_api(&socket_path, "PATCH", "/vm", Some(body)).await?;

        {
            let mut vms = self.vms.lock().await;
            if let Some(instance) = vms.get_mut(vm_id) {
                instance.status = VmStatus::Running;
            }
        }

        info!("FC: VM {} resumed", vm_id);
        Ok(())
    }

    async fn delete_vm(&self, vm_id: &str) -> Result<(), VmmError> {
        info!("FC: Deleting VM {}", vm_id);

        let mut instance = {
            let mut vms = self.vms.lock().await;
            vms.remove(vm_id)
                .ok_or_else(|| VmmError::VmNotFound(vm_id.to_string()))?
        };

        if let Some(mut process) = instance.process.take()
            && let Err(e) = process.kill().await
        {
            warn!("FC: Failed to kill process for VM {}: {}", vm_id, e);
        }

        if instance.socket_path.exists() {
            let _ = tokio::fs::remove_file(&instance.socket_path).await;
        }

        let config_path = self.config_path(vm_id);
        if config_path.exists() {
            let _ = tokio::fs::remove_file(&config_path).await;
        }

        let seed_path = self.cloud_init_seed_path(vm_id);
        if tokio::fs::try_exists(&seed_path).await.unwrap_or(false) {
            let _ = tokio::fs::remove_file(&seed_path).await;
        }

        for tap in &instance.tap_devices {
            Self::delete_tap_device(tap).await;
        }

        info!("FC: VM {} deleted", vm_id);
        Ok(())
    }

    async fn get_vm_info(&self, vm_id: &str) -> Result<VmState, VmmError> {
        let vms = self.vms.lock().await;
        let instance = vms
            .get(vm_id)
            .ok_or_else(|| VmmError::VmNotFound(vm_id.to_string()))?;

        // Trust explicit Shutdown — the FC process may still be responding on
        // the socket briefly after stop_vm (e.g. paused VM sent Ctrl+Alt+Del).
        if instance.status == VmStatus::Shutdown {
            return Ok(instance.to_vm_state());
        }

        let mut state = instance.to_vm_state();

        // Try to get live status from the Firecracker instance-info endpoint.
        if let Ok(body) = Self::fc_api(&instance.socket_path, "GET", "/", None).await
            && let Ok(info) = serde_json::from_str::<serde_json::Value>(&body)
            && let Some(s) = info.get("state").and_then(|v| v.as_str())
        {
            state.status = match s {
                "Running" => VmStatus::Running.into(),
                "Paused" => VmStatus::Paused.into(),
                "Not started" => VmStatus::Created.into(),
                _ => VmStatus::Shutdown.into(),
            };
        }

        Ok(state)
    }

    async fn list_vms(&self) -> Vec<VmState> {
        let vms = self.vms.lock().await;
        vms.values().map(|i| i.to_vm_state()).collect()
    }

    async fn snapshot_vm(&self, vm_id: &str, destination_url: &str) -> Result<(), VmmError> {
        info!("FC: Snapshotting VM {} to {}", vm_id, destination_url);

        let (socket_path, already_paused) = {
            let vms = self.vms.lock().await;
            let instance = vms
                .get(vm_id)
                .ok_or_else(|| VmmError::VmNotFound(vm_id.to_string()))?;
            (
                instance.socket_path.clone(),
                instance.status == VmStatus::Paused,
            )
        };

        // Firecracker requires the VM to be paused before taking a snapshot.
        // Skip if the caller already paused it to avoid a 400 from FC.
        if !already_paused {
            self.pause_vm(vm_id).await?;
        }

        let dest = destination_url
            .strip_prefix("file://")
            .unwrap_or(destination_url);

        tokio::fs::create_dir_all(dest).await.map_err(|e| {
            VmmError::ProcessError(format!(
                "Failed to create snapshot directory {}: {}",
                dest, e
            ))
        })?;

        let mem_path = format!("{}/mem.snap", dest);
        let state_path = format!("{}/vm.snap", dest);

        let body = serde_json::json!({
            "snapshot_type": "Full",
            "snapshot_path": state_path,
            "mem_file_path": mem_path
        });
        Self::fc_api(
            &socket_path,
            "PUT",
            "/snapshot/create",
            Some(&body.to_string()),
        )
        .await?;

        info!("FC: VM {} snapshotted to {}", vm_id, destination_url);
        Ok(())
    }

    async fn restore_vm(&self, vm_id: &str, source_url: &str) -> Result<(), VmmError> {
        info!("FC: Restoring VM {} from {}", vm_id, source_url);

        // Clean up any existing instance.
        {
            let mut vms = self.vms.lock().await;
            if let Some(mut instance) = vms.remove(vm_id) {
                if let Some(mut process) = instance.process.take() {
                    let _ = process.kill().await;
                }
                if instance.socket_path.exists() {
                    let _ = tokio::fs::remove_file(&instance.socket_path).await;
                }
            }
        }

        tokio::fs::create_dir_all(&self.runtime_dir)
            .await
            .map_err(VmmError::SpawnError)?;

        let socket_path = self.socket_path(vm_id);
        let log_path = self.log_path(vm_id);

        if socket_path.exists() {
            let _ = tokio::fs::remove_file(&socket_path).await;
        }

        let log_file = tokio::fs::File::create(&log_path)
            .await
            .map_err(VmmError::SpawnError)?
            .into_std()
            .await;
        let stderr_file = log_file.try_clone().map_err(VmmError::SpawnError)?;

        let process = Command::new(&self.fc_binary)
            .arg("--api-sock")
            .arg(&socket_path)
            .stdout(std::process::Stdio::from(log_file))
            .stderr(std::process::Stdio::from(stderr_file))
            .kill_on_drop(true)
            .spawn()
            .map_err(VmmError::SpawnError)?;

        Self::wait_for_socket(&socket_path).await?;

        let src = source_url.strip_prefix("file://").unwrap_or(source_url);
        let mem_path = format!("{}/mem.snap", src);
        let state_path = format!("{}/vm.snap", src);

        let body = serde_json::json!({
            "snapshot_path": state_path,
            "mem_backend": {
                "backend_path": mem_path,
                "backend_type": "File"
            },
            "enable_diff_snapshots": false,
            "resume_vm": true
        });
        if let Err(e) = Self::fc_api(
            &socket_path,
            "PUT",
            "/snapshot/load",
            Some(&body.to_string()),
        )
        .await
        {
            let _ = tokio::fs::remove_file(&socket_path).await;
            return Err(e);
        }

        // Load persisted config for the restored VM if available.
        let proto_config =
            self.load_persisted_config(vm_id)
                .await?
                .unwrap_or_else(|| ProtoVmConfig {
                    vm_id: vm_id.to_string(),
                    ..Default::default()
                });

        let instance = FcVmInstance {
            proto_config,
            process: Some(process),
            socket_path,
            status: VmStatus::Running,
            tap_devices: vec![],
        };

        {
            let mut vms = self.vms.lock().await;
            vms.insert(vm_id.to_string(), instance);
        }

        info!("FC: VM {} restored successfully from {}", vm_id, source_url);
        Ok(())
    }

    async fn recover_vms(&self) {
        info!(
            "FC: Scanning for surviving Firecracker processes in {:?}",
            self.runtime_dir
        );

        let mut read_dir = match tokio::fs::read_dir(&self.runtime_dir).await {
            Ok(rd) => rd,
            Err(e) => {
                warn!("FC: Failed to read runtime dir for recovery: {}", e);
                return;
            }
        };

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let path = entry.path();
            // FC sockets are named {vm_id}.fc.sock
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if !name.ends_with(".fc.sock") {
                continue;
            }
            let vm_id = name.trim_end_matches(".fc.sock").to_string();

            let proto_config = match self.load_persisted_config(&vm_id).await {
                Ok(Some(c)) => c,
                Ok(None) => continue,
                Err(e) => {
                    warn!("FC: Failed to load config for VM {}: {}", vm_id, e);
                    continue;
                }
            };

            let tap_devices: Vec<String> = proto_config
                .networks
                .iter()
                .filter_map(|n| n.tap.clone())
                .filter(|t| t.starts_with("qf"))
                .collect();

            // Try to connect to get the current status.
            let status = match Self::fc_api(&path, "GET", "/", None).await {
                Ok(body) => {
                    if let Ok(info) = serde_json::from_str::<serde_json::Value>(&body) {
                        match info.get("state").and_then(|v| v.as_str()) {
                            Some("Running") => VmStatus::Running,
                            Some("Paused") => VmStatus::Paused,
                            _ => VmStatus::Unknown,
                        }
                    } else {
                        VmStatus::Unknown
                    }
                }
                Err(_) => VmStatus::Unknown,
            };

            let instance = FcVmInstance {
                proto_config,
                process: None,
                socket_path: path.clone(),
                status,
                tap_devices,
            };

            let mut vms = self.vms.lock().await;
            vms.insert(vm_id.clone(), instance);
            info!("FC: Recovered VM {} with status {:?}", vm_id, status);
        }
    }

    fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    async fn is_vm_process_alive(&self, vm_id: &str) -> bool {
        let mut vms = self.vms.lock().await;
        let Some(instance) = vms.get_mut(vm_id) else {
            return false;
        };
        match &mut instance.process {
            Some(child) => child.try_wait().ok().flatten().is_none(),
            None => instance.socket_path.exists(),
        }
    }
}
