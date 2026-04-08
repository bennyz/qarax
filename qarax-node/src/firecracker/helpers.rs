use super::*;

impl FirecrackerManager {
    pub(super) fn socket_path(&self, vm_id: &str) -> PathBuf {
        self.runtime_dir.join(format!("{}.fc.sock", vm_id))
    }

    pub(super) fn log_path(&self, vm_id: &str) -> PathBuf {
        self.runtime_dir.join(format!("{}.fc.log", vm_id))
    }

    pub(super) fn config_path(&self, vm_id: &str) -> PathBuf {
        self.runtime_dir.join(format!("{}.fc.json", vm_id))
    }

    pub(super) fn cloud_init_seed_path(&self, vm_id: &str) -> PathBuf {
        self.runtime_dir.join(format!("{}-fc-cidata.img", vm_id))
    }

    pub(super) fn tap_name_for_net(vm_id: &str, net_index: usize) -> String {
        let hex: String = vm_id
            .chars()
            .filter(|c| c.is_ascii_hexdigit())
            .take(8)
            .collect();
        format!("qf{}n{}", hex, net_index)
    }

    pub(super) async fn create_tap_device(name: &str) -> Result<(), VmmError> {
        let add = Command::new("ip")
            .args(["tuntap", "add", name, "mode", "tap"])
            .status()
            .await
            .map_err(|e| VmmError::TapError(format!("failed to run ip tuntap add: {e}")))?;
        if !add.success() {
            return Err(VmmError::TapError(format!(
                "ip tuntap add {name} failed with status {add}"
            )));
        }

        let up = Command::new("ip")
            .args(["link", "set", name, "up"])
            .status()
            .await
            .map_err(|e| VmmError::TapError(format!("failed to run ip link set up: {e}")))?;
        if !up.success() {
            return Err(VmmError::TapError(format!(
                "ip link set {name} up failed with status {up}"
            )));
        }

        info!("FC TAP device {} created and up", name);
        Ok(())
    }

    pub(super) async fn delete_tap_device(name: &str) {
        match Command::new("ip")
            .args(["link", "delete", name])
            .status()
            .await
        {
            Ok(s) if s.success() => info!("FC TAP device {} deleted", name),
            Ok(s) => warn!("ip link delete {} failed with status {}", name, s),
            Err(e) => warn!("Failed to run ip link delete {}: {}", name, e),
        }
    }

    pub(super) async fn wait_for_socket(socket_path: &PathBuf) -> Result<(), VmmError> {
        for _ in 0..50 {
            match UnixStream::connect(socket_path).await {
                Ok(_) => return Ok(()),
                Err(_) => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
        Err(VmmError::SpawnError(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("Firecracker socket {} not ready", socket_path.display()),
        )))
    }

    /// Send a raw HTTP/1.1 request over a Unix socket to the Firecracker API.
    pub(super) async fn fc_api(
        socket_path: &PathBuf,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<String, VmmError> {
        let stream = UnixStream::connect(socket_path)
            .await
            .map_err(VmmError::SpawnError)?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| VmmError::ProcessError(e.to_string()))?;

        tokio::spawn(conn);

        let request = if let Some(body_str) = body {
            let body_bytes = Bytes::from(body_str.to_string());
            Request::builder()
                .method(method)
                .uri(format!("http://localhost{}", path))
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .body(BoxBody::new(Full::new(body_bytes)))
                .map_err(|e| VmmError::ProcessError(e.to_string()))?
        } else {
            Request::builder()
                .method(method)
                .uri(format!("http://localhost{}", path))
                .header("Accept", "application/json")
                .body(BoxBody::new(Empty::new()))
                .map_err(|e| VmmError::ProcessError(e.to_string()))?
        };

        let response = sender
            .send_request(request)
            .await
            .map_err(|e| VmmError::ProcessError(e.to_string()))?;

        let status = response.status();
        let mut body_stream = http_body_util::BodyStream::new(response.into_body());
        let mut bytes = bytes::BytesMut::new();
        while let Some(chunk) = body_stream.next().await {
            if let Ok(chunk) = chunk
                && let Ok(data) = chunk.into_data()
            {
                bytes.extend_from_slice(&data);
            }
        }
        let body = String::from_utf8_lossy(&bytes).to_string();

        if !status.is_success() {
            return Err(VmmError::ProcessError(format!(
                "FC API {} {} failed: HTTP {} — {}",
                method, path, status, body
            )));
        }

        Ok(body)
    }

    /// Configure a freshly-spawned Firecracker instance from a proto VmConfig.
    pub(super) async fn configure_vm(
        socket_path: &PathBuf,
        config: &ProtoVmConfig,
    ) -> Result<(), VmmError> {
        // Machine config: vCPUs and memory.
        let cpus = config.cpus.as_ref().map(|c| c.boot_vcpus).unwrap_or(1);
        let mem_mib = config
            .memory
            .as_ref()
            .map(|m| m.size / (1024 * 1024))
            .unwrap_or(128);
        let machine_config = serde_json::json!({
            "vcpu_count": cpus,
            "mem_size_mib": mem_mib
        });
        Self::fc_api(
            socket_path,
            "PUT",
            "/machine-config",
            Some(&machine_config.to_string()),
        )
        .await?;

        // Boot source: kernel + cmdline + initrd.
        if let Some(payload) = &config.payload {
            let kernel_path = payload.kernel.as_deref().unwrap_or("");
            if !kernel_path.is_empty() {
                let mut boot = serde_json::json!({
                    "kernel_image_path": kernel_path
                });
                if let Some(cmdline) = &payload.cmdline
                    && !cmdline.is_empty()
                {
                    boot["boot_args"] = serde_json::Value::String(cmdline.clone());
                }
                if let Some(initramfs) = &payload.initramfs
                    && !initramfs.is_empty()
                {
                    boot["initrd_path"] = serde_json::Value::String(initramfs.clone());
                }
                Self::fc_api(socket_path, "PUT", "/boot-source", Some(&boot.to_string())).await?;
            }
        }

        // Drives (disks). Firecracker identifies drives by a string ID.
        // The first drive that is not read-only is the root device.
        let mut has_root = false;
        for disk in &config.disks {
            if let Some(path) = &disk.path {
                let readonly = disk.readonly.unwrap_or(false);
                let is_root = !readonly && !has_root;
                if is_root {
                    has_root = true;
                }
                let drive = serde_json::json!({
                    "drive_id": disk.id,
                    "path_on_host": path,
                    "is_root_device": is_root,
                    "is_read_only": readonly
                });
                Self::fc_api(
                    socket_path,
                    "PUT",
                    &format!("/drives/{}", disk.id),
                    Some(&drive.to_string()),
                )
                .await?;
                debug!("FC drive {} configured (root={})", disk.id, is_root);
            }
        }

        // Network interfaces.
        for net in &config.networks {
            if let Some(tap) = &net.tap {
                let mut iface = serde_json::json!({
                    "iface_id": net.id,
                    "host_dev_name": tap
                });
                if let Some(mac) = &net.mac {
                    iface["guest_mac"] = serde_json::Value::String(mac.clone());
                }
                Self::fc_api(
                    socket_path,
                    "PUT",
                    &format!("/network-interfaces/{}", net.id),
                    Some(&iface.to_string()),
                )
                .await?;
                debug!("FC network interface {} configured (tap={})", net.id, tap);
            }
        }

        Ok(())
    }

    pub(super) async fn load_persisted_config(
        &self,
        vm_id: &str,
    ) -> Result<Option<ProtoVmConfig>, VmmError> {
        let config_path = self.config_path(vm_id);
        if !config_path.exists() {
            return Ok(None);
        }
        let bytes = tokio::fs::read(&config_path)
            .await
            .map_err(VmmError::SpawnError)?;
        let config = ProtoVmConfig::decode(bytes.as_slice()).map_err(|e| {
            VmmError::InvalidConfig(format!("Failed to decode FC config for {}: {}", vm_id, e))
        })?;
        Ok(Some(config))
    }
}
