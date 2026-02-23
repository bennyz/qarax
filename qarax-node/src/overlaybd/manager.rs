//! OverlayBD manager for lazy block-level OCI image loading.
//!
//! This module manages the lifecycle of OverlayBD block devices:
//! - Converting OCI images to OverlayBD format via `obdconv`
//! - Mounting images as block devices via overlaybd-tcmu (by writing TCMU config JSON)
//! - Unmounting by removing the per-VM config directory

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum OverlayBdError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("obdconv failed: {0}")]
    ObdConvFailed(String),

    #[error("Timeout waiting for overlaybd-tcmu to create device")]
    MountTimeout,

    #[error("Invalid image reference: {0}")]
    InvalidImageRef(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Represents a successfully mounted OverlayBD block device.
#[derive(Debug, Clone)]
pub struct MountedDevice {
    /// Host device path, e.g. "/dev/sdb"
    pub device_path: String,
    /// Per-VM config directory (cleaned up on unmount)
    pub config_dir: PathBuf,
}

/// Manages OverlayBD block devices for VMs.
pub struct OverlayBdManager {
    /// Path to the `obdconv` binary
    obdconv_binary: PathBuf,
    /// Base cache directory, default: /var/lib/qarax/overlaybd/
    cache_dir: PathBuf,
    /// Currently mounted devices, keyed by VM ID
    mounts: Arc<Mutex<HashMap<String, MountedDevice>>>,
}

impl OverlayBdManager {
    pub fn new(obdconv_binary: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        let obdconv_binary = obdconv_binary.into();
        let cache_dir = cache_dir.into();
        info!(
            "OverlayBdManager initialized: obdconv={}, cache_dir={}",
            obdconv_binary.display(),
            cache_dir.display()
        );
        Self {
            obdconv_binary,
            cache_dir,
            mounts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Import an OCI image: convert to OverlayBD format via `obdconv` and push to the target
    /// registry. Idempotent — if the converted image already exists in the registry this is a no-op.
    ///
    /// Returns the image reference in the target registry.
    pub async fn import_image(
        &self,
        image_ref: &str,
        registry_url: &str,
    ) -> Result<String, OverlayBdError> {
        // Derive the target ref: strip protocol + host from registry_url if needed and combine.
        let target_ref = build_target_ref(image_ref, registry_url)?;

        info!("Importing OverlayBD image: {} -> {}", image_ref, target_ref);

        let status = tokio::process::Command::new(&self.obdconv_binary)
            .args(["convert", image_ref, &target_ref])
            .status()
            .await
            .map_err(|e| {
                OverlayBdError::ObdConvFailed(format!("Failed to spawn obdconv: {}", e))
            })?;

        if !status.success() {
            return Err(OverlayBdError::ObdConvFailed(format!(
                "obdconv convert exited with status {}",
                status
            )));
        }

        info!("OverlayBD image imported: {}", target_ref);
        Ok(target_ref)
    }

    /// Mount an OverlayBD image as a block device for the given VM.
    ///
    /// Writes a TCMU config JSON file that overlaybd-tcmu daemon picks up automatically,
    /// then polls the result file until the device path appears (up to 30 seconds).
    pub async fn mount(
        &self,
        vm_id: &str,
        image_ref: &str,
        registry_url: &str,
    ) -> Result<MountedDevice, OverlayBdError> {
        let config_dir = self.cache_dir.join(vm_id);
        tokio::fs::create_dir_all(&config_dir).await?;

        let result_file = config_dir.join("result");
        let upper_index = config_dir.join("upper.index");
        let upper_data = config_dir.join("upper.data");
        let config_file = config_dir.join("config.json");

        // Build the repo blob URL from registry_url and the image name
        let (repo_name, _tag) = parse_image_ref(image_ref)?;
        let repo_blob_url = format!(
            "{}/v2/{}/blobs/",
            registry_url.trim_end_matches('/'),
            repo_name
        );

        // Build TCMU config JSON.
        // overlaybd-tcmu watches /etc/overlaybd/registry/ for JSON files matching this structure.
        let config = serde_json::json!({
            "repoBlobUrl": repo_blob_url,
            "lowers": [],          // overlaybd-tcmu resolves layers from the registry manifest
            "upper": {
                "index": upper_index.to_string_lossy(),
                "data":  upper_data.to_string_lossy()
            },
            "resultFile": result_file.to_string_lossy(),
            "imageRef": image_ref,
            "registryUrl": registry_url
        });

        let config_json = serde_json::to_string_pretty(&config)?;
        tokio::fs::write(&config_file, &config_json).await?;

        info!(
            "Wrote OverlayBD TCMU config for VM {} at {}",
            vm_id,
            config_file.display()
        );

        // Poll the result file until overlaybd-tcmu writes the device path (max 30s)
        let device_path = self.wait_for_device(&result_file).await?;

        info!("OverlayBD device for VM {}: {}", vm_id, device_path);

        let mounted = MountedDevice {
            device_path,
            config_dir: config_dir.clone(),
        };

        self.mounts
            .lock()
            .await
            .insert(vm_id.to_string(), mounted.clone());

        Ok(mounted)
    }

    /// Unmount the OverlayBD device for the given VM by removing its config directory.
    /// overlaybd-tcmu detects the removal and detaches the device automatically.
    pub async fn unmount(&self, vm_id: &str) {
        let config_dir = {
            let mut mounts = self.mounts.lock().await;
            mounts.remove(vm_id).map(|m| m.config_dir)
        };

        let dir = config_dir.unwrap_or_else(|| self.cache_dir.join(vm_id));

        if dir.exists() {
            if let Err(e) = tokio::fs::remove_dir_all(&dir).await {
                warn!(
                    "Failed to remove OverlayBD config dir for VM {}: {}",
                    vm_id, e
                );
            } else {
                info!("OverlayBD config dir removed for VM {}", vm_id);
            }
        }
    }

    /// Scan the cache directory on startup and rebuild the mounts map from any existing
    /// `result` files left over from a previous run.
    pub async fn recover(&self) {
        let mut read_dir = match tokio::fs::read_dir(&self.cache_dir).await {
            Ok(rd) => rd,
            Err(_) => return,
        };

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let vm_id = match entry.file_name().into_string() {
                Ok(s) => s,
                Err(_) => continue,
            };

            let result_file = entry.path().join("result");
            if !result_file.exists() {
                continue;
            }

            let device_path = match tokio::fs::read_to_string(&result_file).await {
                Ok(s) => s.trim().to_string(),
                Err(_) => continue,
            };

            if device_path.is_empty() {
                continue;
            }

            info!(
                "Recovered OverlayBD mount for VM {}: {}",
                vm_id, device_path
            );

            let mounted = MountedDevice {
                device_path,
                config_dir: entry.path(),
            };
            self.mounts.lock().await.insert(vm_id, mounted);
        }
    }

    /// Poll the result file until overlaybd-tcmu writes a non-empty device path.
    /// Times out after 30 seconds (300 × 100ms).
    async fn wait_for_device(&self, result_file: &PathBuf) -> Result<String, OverlayBdError> {
        for _ in 0..300 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            if result_file.exists() {
                let content = tokio::fs::read_to_string(result_file).await?;
                let device = content.trim().to_string();
                if !device.is_empty() {
                    return Ok(device);
                }
            }
        }

        Err(OverlayBdError::MountTimeout)
    }
}

/// Build the target image reference in the user's registry.
/// e.g. image_ref = "ubuntu:22.04", registry_url = "http://localhost:5000"
/// -> "localhost:5000/ubuntu:22.04"
fn build_target_ref(image_ref: &str, registry_url: &str) -> Result<String, OverlayBdError> {
    // Strip scheme from registry_url
    let host = registry_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');

    if host.is_empty() {
        return Err(OverlayBdError::InvalidImageRef(format!(
            "Empty registry host in URL: {}",
            registry_url
        )));
    }

    // If image_ref already has a registry prefix, strip it.
    // Otherwise just prefix with the host.
    let bare = if image_ref.contains('/') {
        // e.g. "docker.io/library/ubuntu:22.04" -> "library/ubuntu:22.04"
        // or "myregistry/ubuntu:22.04" -> "ubuntu:22.04" if it contains the same host
        image_ref.split_once('/').map(|x| x.1).unwrap_or(image_ref)
    } else {
        image_ref
    };

    Ok(format!("{}/{}", host, bare))
}

/// Parse an image reference into (repo_name, tag).
/// e.g. "ubuntu:22.04" -> ("ubuntu", "22.04")
/// e.g. "docker.io/library/ubuntu:22.04" -> ("library/ubuntu", "22.04")
fn parse_image_ref(image_ref: &str) -> Result<(String, String), OverlayBdError> {
    // Strip registry host if present
    let without_registry = if image_ref.contains('/') {
        let parts: Vec<&str> = image_ref.splitn(2, '/').collect();
        // Heuristic: if first part contains a '.' or ':' it's a registry
        if parts[0].contains('.') || parts[0].contains(':') {
            parts[1]
        } else {
            image_ref
        }
    } else {
        image_ref
    };

    let (repo, tag) = match without_registry.rsplit_once(':') {
        Some((r, t)) => (r.to_string(), t.to_string()),
        None => (without_registry.to_string(), "latest".to_string()),
    };

    if repo.is_empty() {
        return Err(OverlayBdError::InvalidImageRef(format!(
            "Empty repository in: {}",
            image_ref
        )));
    }

    Ok((repo, tag))
}
