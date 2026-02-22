use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tonic::{Request, Response, Status};
use tracing::{debug, error, info, warn};

use crate::cloud_hypervisor::VmManager;
use crate::rpc::node::{
    AddDiskDeviceRequest, AddFsDeviceRequest, AddNetworkDeviceRequest, ConsoleInput,
    ConsoleLogResponse, ConsoleOutput, ConsolePtyPathResponse, DeviceCounters, NodeInfo,
    OciImageRequest, OciImageResponse, RemoveDeviceRequest, VmConfig, VmCounters, VmId, VmList,
    VmState, vm_service_server::VmService,
};

/// Implementation of VmService using Cloud Hypervisor
#[derive(Clone)]
pub struct VmServiceImpl {
    manager: Arc<VmManager>,
}

impl VmServiceImpl {
    /// Create a new VmServiceImpl with default paths (no Nydus support)
    pub async fn new() -> Self {
        Self::with_paths("/var/lib/qarax/vms", "/usr/local/bin/cloud-hypervisor").await
    }

    /// Create a new VmServiceImpl with custom paths (no Nydus support)
    pub async fn with_paths(
        runtime_dir: impl Into<std::path::PathBuf>,
        ch_binary: impl Into<std::path::PathBuf>,
    ) -> Self {
        let manager = Arc::new(VmManager::new(runtime_dir, ch_binary, None));
        manager.recover_vms().await;
        Self { manager }
    }

    /// Create from an existing VmManager
    pub fn from_manager(manager: Arc<VmManager>) -> Self {
        Self { manager }
    }
}

#[tonic::async_trait]
impl VmService for VmServiceImpl {
    async fn create_vm(&self, request: Request<VmConfig>) -> Result<Response<VmState>, Status> {
        let config = request.into_inner();
        let vm_id = config.vm_id.clone();

        info!("Creating VM: {}", vm_id);
        debug!("VM config: {:?}", config);

        match self.manager.create_vm(config).await {
            Ok(state) => {
                info!("VM {} created successfully", vm_id);
                Ok(Response::new(state))
            }
            Err(e) => {
                error!("Failed to create VM {}: {}", vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn start_vm(&self, request: Request<VmId>) -> Result<Response<()>, Status> {
        let vm_id = request.into_inner().id;
        info!("Starting VM: {}", vm_id);

        match self.manager.start_vm(&vm_id).await {
            Ok(()) => {
                info!("VM {} started successfully", vm_id);
                Ok(Response::new(()))
            }
            Err(e) => {
                error!("Failed to start VM {}: {}", vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn stop_vm(&self, request: Request<VmId>) -> Result<Response<()>, Status> {
        let vm_id = request.into_inner().id;
        info!("Stopping VM: {}", vm_id);

        match self.manager.stop_vm(&vm_id).await {
            Ok(()) => {
                info!("VM {} stopped successfully", vm_id);
                Ok(Response::new(()))
            }
            Err(e) => {
                error!("Failed to stop VM {}: {}", vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn pause_vm(&self, request: Request<VmId>) -> Result<Response<()>, Status> {
        let vm_id = request.into_inner().id;
        info!("Pausing VM: {}", vm_id);

        match self.manager.pause_vm(&vm_id).await {
            Ok(()) => {
                info!("VM {} paused successfully", vm_id);
                Ok(Response::new(()))
            }
            Err(e) => {
                error!("Failed to pause VM {}: {}", vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn resume_vm(&self, request: Request<VmId>) -> Result<Response<()>, Status> {
        let vm_id = request.into_inner().id;
        info!("Resuming VM: {}", vm_id);

        match self.manager.resume_vm(&vm_id).await {
            Ok(()) => {
                info!("VM {} resumed successfully", vm_id);
                Ok(Response::new(()))
            }
            Err(e) => {
                error!("Failed to resume VM {}: {}", vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn delete_vm(&self, request: Request<VmId>) -> Result<Response<()>, Status> {
        let vm_id = request.into_inner().id;
        info!("Deleting VM: {}", vm_id);

        match self.manager.delete_vm(&vm_id).await {
            Ok(()) => {
                info!("VM {} deleted successfully", vm_id);
                Ok(Response::new(()))
            }
            Err(e) => {
                error!("Failed to delete VM {}: {}", vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn get_vm_info(&self, request: Request<VmId>) -> Result<Response<VmState>, Status> {
        let vm_id = request.into_inner().id;
        info!("Getting VM info: {}", vm_id);

        match self.manager.get_vm_info(&vm_id).await {
            Ok(state) => Ok(Response::new(state)),
            Err(e) => {
                error!("Failed to get VM info {}: {}", vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn list_vms(&self, _request: Request<()>) -> Result<Response<VmList>, Status> {
        info!("Listing VMs");

        let vms = self.manager.list_vms().await;
        info!("Found {} VMs", vms.len());
        Ok(Response::new(VmList { vms }))
    }

    async fn get_vm_counters(
        &self,
        request: Request<VmId>,
    ) -> Result<Response<VmCounters>, Status> {
        let vm_id = request.into_inner().id;
        info!("Getting VM counters: {}", vm_id);

        match self.manager.get_vm_counters(&vm_id).await {
            Ok(counters) => {
                let proto_counters = counters
                    .into_iter()
                    .map(|(device, values)| (device, DeviceCounters { values }))
                    .collect();
                Ok(Response::new(VmCounters {
                    counters: proto_counters,
                }))
            }
            Err(e) => {
                error!("Failed to get VM counters {}: {}", vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn add_network_device(
        &self,
        request: Request<AddNetworkDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!("Adding network device to VM: {}", req.vm_id);

        let config = req
            .config
            .ok_or_else(|| Status::invalid_argument("Missing network config"))?;

        match self.manager.add_network_device(&req.vm_id, &config).await {
            Ok(()) => {
                info!("Network device added to VM {}", req.vm_id);
                Ok(Response::new(()))
            }
            Err(e) => {
                error!("Failed to add network device to VM {}: {}", req.vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn remove_network_device(
        &self,
        request: Request<RemoveDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!(
            "Removing network device {} from VM: {}",
            req.device_id, req.vm_id
        );

        match self
            .manager
            .remove_network_device(&req.vm_id, &req.device_id)
            .await
        {
            Ok(()) => {
                info!(
                    "Network device {} removed from VM {}",
                    req.device_id, req.vm_id
                );
                Ok(Response::new(()))
            }
            Err(e) => {
                error!(
                    "Failed to remove network device {} from VM {}: {}",
                    req.device_id, req.vm_id, e
                );
                Err(map_manager_error(e))
            }
        }
    }

    async fn add_disk_device(
        &self,
        request: Request<AddDiskDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!("Adding disk device to VM: {}", req.vm_id);

        let config = req
            .config
            .ok_or_else(|| Status::invalid_argument("Missing disk config"))?;

        match self.manager.add_disk_device(&req.vm_id, &config).await {
            Ok(()) => {
                info!("Disk device added to VM {}", req.vm_id);
                Ok(Response::new(()))
            }
            Err(e) => {
                error!("Failed to add disk device to VM {}: {}", req.vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn remove_disk_device(
        &self,
        request: Request<RemoveDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!(
            "Removing disk device {} from VM: {}",
            req.device_id, req.vm_id
        );

        match self
            .manager
            .remove_disk_device(&req.vm_id, &req.device_id)
            .await
        {
            Ok(()) => {
                info!(
                    "Disk device {} removed from VM {}",
                    req.device_id, req.vm_id
                );
                Ok(Response::new(()))
            }
            Err(e) => {
                error!(
                    "Failed to remove disk device {} from VM {}: {}",
                    req.device_id, req.vm_id, e
                );
                Err(map_manager_error(e))
            }
        }
    }

    async fn add_fs_device(
        &self,
        request: Request<AddFsDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!("Adding fs device to VM: {}", req.vm_id);

        let config = req
            .config
            .ok_or_else(|| Status::invalid_argument("Missing fs config"))?;

        match self.manager.add_fs_device(&req.vm_id, &config).await {
            Ok(()) => {
                info!("Fs device added to VM {}", req.vm_id);
                Ok(Response::new(()))
            }
            Err(e) => {
                error!("Failed to add fs device to VM {}: {}", req.vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    async fn remove_fs_device(
        &self,
        request: Request<RemoveDeviceRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.into_inner();
        info!(
            "Removing fs device {} from VM: {}",
            req.device_id, req.vm_id
        );

        match self
            .manager
            .remove_fs_device(&req.vm_id, &req.device_id)
            .await
        {
            Ok(()) => {
                info!("Fs device {} removed from VM {}", req.device_id, req.vm_id);
                Ok(Response::new(()))
            }
            Err(e) => {
                error!(
                    "Failed to remove fs device {} from VM {}: {}",
                    req.device_id, req.vm_id, e
                );
                Err(map_manager_error(e))
            }
        }
    }

    async fn pull_image(
        &self,
        request: Request<OciImageRequest>,
    ) -> Result<Response<OciImageResponse>, Status> {
        let image_ref = request.into_inner().image_ref;
        info!("Pulling OCI image: {}", image_ref);

        let store = self
            .manager
            .image_store_manager()
            .ok_or_else(|| Status::unimplemented("virtiofsd not configured on this node"))?;

        match store.pull_and_unpack(&image_ref).await {
            Ok(info) => Ok(Response::new(OciImageResponse {
                image_ref: info.image_ref,
                digest: info.digest,
                bootstrap_path: info.rootfs_path.to_string_lossy().to_string(),
                socket_path: String::new(),
                available: true,
            })),
            Err(e) => {
                error!("Failed to pull image {}: {}", image_ref, e);
                Err(Status::internal(format!("Pull failed: {}", e)))
            }
        }
    }

    async fn get_image_status(
        &self,
        request: Request<OciImageRequest>,
    ) -> Result<Response<OciImageResponse>, Status> {
        let image_ref = request.into_inner().image_ref;
        info!("Getting image status: {}", image_ref);

        let store = self
            .manager
            .image_store_manager()
            .ok_or_else(|| Status::unimplemented("virtiofsd not configured on this node"))?;

        match store.get_image_status(&image_ref) {
            Some(info) => Ok(Response::new(OciImageResponse {
                image_ref: info.image_ref,
                digest: info.digest,
                bootstrap_path: info.rootfs_path.to_string_lossy().to_string(),
                socket_path: String::new(),
                available: true,
            })),
            None => Ok(Response::new(OciImageResponse {
                image_ref,
                digest: String::new(),
                bootstrap_path: String::new(),
                socket_path: String::new(),
                available: false,
            })),
        }
    }

    async fn read_console_log(
        &self,
        request: Request<VmId>,
    ) -> Result<Response<ConsoleLogResponse>, Status> {
        let vm_id = request.into_inner().id;
        info!("Reading console log for VM: {}", vm_id);

        let log_path = self
            .manager
            .runtime_dir()
            .join(format!("{}.console.log", vm_id));

        if !log_path.exists() {
            return Ok(Response::new(ConsoleLogResponse {
                content: String::new(),
                available: false,
            }));
        }

        match tokio::fs::read_to_string(&log_path).await {
            Ok(content) => Ok(Response::new(ConsoleLogResponse {
                content,
                available: true,
            })),
            Err(e) => {
                error!("Failed to read console log for VM {}: {}", vm_id, e);
                Err(Status::internal(format!(
                    "Failed to read console log: {}",
                    e
                )))
            }
        }
    }

    async fn get_node_info(&self, _request: Request<()>) -> Result<Response<NodeInfo>, Status> {
        let hostname = gethostname::gethostname().to_string_lossy().into_owned();

        // Get Cloud Hypervisor version
        let ch_version = match tokio::process::Command::new(self.manager.ch_binary())
            .arg("--version")
            .output()
            .await
        {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(e) => {
                error!("Failed to get Cloud Hypervisor version: {}", e);
                "unknown".to_string()
            }
        };

        // Get kernel version (the one we provide for VMs)
        // We can try to extract it from the vmlinux file if we want to be fancy,
        // but for now let's just use the host kernel version as a proxy or just "custom"
        let kernel_version = std::fs::read_to_string("/proc/version")
            .unwrap_or_else(|_| "unknown".to_string())
            .split_whitespace()
            .nth(2)
            .unwrap_or("unknown")
            .to_string();

        Ok(Response::new(NodeInfo {
            hostname,
            cloud_hypervisor_version: ch_version,
            kernel_version,
        }))
    }

    async fn attach_console(
        &self,
        request: Request<tonic::Streaming<ConsoleInput>>,
    ) -> Result<Response<Self::AttachConsoleStream>, Status> {
        let mut input_stream = request.into_inner();

        // Read first message to get VM ID
        let first_msg = input_stream
            .message()
            .await
            .map_err(|e| Status::invalid_argument(format!("Failed to read first message: {}", e)))?
            .ok_or_else(|| Status::invalid_argument("Empty input stream"))?;

        let vm_id = first_msg.vm_id.clone();
        info!("Attaching to console for VM: {}", vm_id);

        // Get PTY path from VM manager
        let pty_path = self
            .manager
            .get_serial_pty_path(&vm_id)
            .await
            .map_err(|e| {
                error!("Failed to get PTY path for VM {}: {}", vm_id, e);
                map_manager_error(e)
            })?
            .ok_or_else(|| {
                Status::failed_precondition("VM console is not configured for PTY mode")
            })?;

        info!("Opening PTY for VM {}: {}", vm_id, pty_path);

        // Open PTY device
        let pty_file = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&pty_path)
            .await
            .map_err(|e| {
                error!("Failed to open PTY {}: {}", pty_path, e);
                Status::internal(format!("Failed to open PTY: {}", e))
            })?;

        let (pty_reader, mut pty_writer) = tokio::io::split(pty_file);

        // Channel for sending output to client
        let (tx, rx) = tokio::sync::mpsc::channel(128);

        // Spawn task to read from PTY and send to client
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let mut reader = tokio::io::BufReader::new(pty_reader);
            let mut buffer = vec![0u8; 4096];

            loop {
                match reader.read(&mut buffer).await {
                    Ok(0) => {
                        debug!("PTY read EOF");
                        break;
                    }
                    Ok(n) => {
                        let output = ConsoleOutput {
                            data: buffer[..n].to_vec(),
                            error: false,
                            error_message: String::new(),
                        };
                        if tx_clone.send(Ok(output)).await.is_err() {
                            debug!("Client disconnected");
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("PTY read error: {}", e);
                        let error_output = ConsoleOutput {
                            data: vec![],
                            error: true,
                            error_message: format!("PTY read error: {}", e),
                        };
                        let _ = tx_clone.send(Ok(error_output)).await;
                        break;
                    }
                }
            }
        });

        // Spawn task to read from client and write to PTY
        tokio::spawn(async move {
            // Process first message if it has data
            if !first_msg.data.is_empty()
                && let Err(e) = pty_writer.write_all(&first_msg.data).await
            {
                warn!("Failed to write to PTY: {}", e);
                return;
            }

            // Process remaining messages
            while let Ok(Some(msg)) = input_stream.message().await {
                if msg.resize {
                    // Handle terminal resize (would need ioctl TIOCSWINSZ)
                    debug!("Terminal resize requested: {}x{}", msg.cols, msg.rows);
                    // TODO: Implement terminal resize using nix crate
                } else if !msg.data.is_empty() {
                    if let Err(e) = pty_writer.write_all(&msg.data).await {
                        warn!("Failed to write to PTY: {}", e);
                        break;
                    }
                    if let Err(e) = pty_writer.flush().await {
                        warn!("Failed to flush PTY: {}", e);
                        break;
                    }
                }
            }
            debug!("Console input stream ended");
        });

        let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::AttachConsoleStream
        ))
    }

    async fn get_console_pty_path(
        &self,
        request: Request<VmId>,
    ) -> Result<Response<ConsolePtyPathResponse>, Status> {
        let vm_id = request.into_inner().id;
        info!("Getting console PTY path for VM: {}", vm_id);

        match self.manager.get_serial_pty_path(&vm_id).await {
            Ok(Some(pty_path)) => Ok(Response::new(ConsolePtyPathResponse {
                pty_path,
                available: true,
            })),
            Ok(None) => Ok(Response::new(ConsolePtyPathResponse {
                pty_path: String::new(),
                available: false,
            })),
            Err(e) => {
                error!("Failed to get PTY path for VM {}: {}", vm_id, e);
                Err(map_manager_error(e))
            }
        }
    }

    type AttachConsoleStream =
        Pin<Box<dyn Stream<Item = Result<ConsoleOutput, Status>> + Send + 'static>>;
}
fn map_manager_error(e: crate::cloud_hypervisor::VmManagerError) -> Status {
    use crate::cloud_hypervisor::VmManagerError;

    match e {
        VmManagerError::VmNotFound(id) => Status::not_found(format!("VM {} not found", id)),
        VmManagerError::VmAlreadyExists(id) => {
            Status::already_exists(format!("VM {} already exists", id))
        }
        VmManagerError::InvalidConfig(msg) => Status::invalid_argument(msg),
        VmManagerError::SpawnError(e) => Status::internal(format!("Failed to spawn CH: {}", e)),
        VmManagerError::SdkError(e) => {
            Status::internal(format!("Cloud Hypervisor SDK error: {}", e))
        }
        VmManagerError::ProcessError(msg) => Status::internal(msg),
        VmManagerError::TapError(msg) => Status::internal(format!("TAP device error: {}", msg)),
    }
}
