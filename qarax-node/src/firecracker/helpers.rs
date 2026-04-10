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
