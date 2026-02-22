use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::{Level, info};

use qarax_node::cloud_hypervisor::VmManager;
use qarax_node::image_store::ImageStoreManager;
use qarax_node::rpc::node::file_transfer_service_server::FileTransferServiceServer;
use qarax_node::rpc::node::vm_service_server::VmServiceServer;
use qarax_node::services::file_transfer::FileTransferServiceImpl;
use qarax_node::services::vm::VmServiceImpl;

#[derive(Parser, Debug)]
#[clap(
    name = "qarax-node",
    about = "qarax data plane - manages VMs using Cloud Hypervisor",
    rename_all = "kebab-case",
    rename_all_env = "screaming-snake"
)]
pub struct Args {
    /// Port to listen on
    #[clap(short, long, default_value = "50051")]
    port: u16,

    /// Runtime directory for VM sockets and logs
    #[clap(long, default_value = "/var/lib/qarax/vms")]
    runtime_dir: PathBuf,

    /// Path to cloud-hypervisor binary
    #[clap(long, default_value = "/usr/local/bin/cloud-hypervisor")]
    cloud_hypervisor_binary: PathBuf,

    /// Path to virtiofsd binary
    #[clap(long, default_value = "/usr/local/bin/virtiofsd")]
    virtiofsd_binary: PathBuf,

    /// Path to qarax-init binary (injected into OCI VMs as the init process)
    #[clap(long, default_value = "/usr/local/bin/qarax-init")]
    qarax_init_binary: PathBuf,

    /// Directory for OCI image cache
    #[clap(long, default_value = "/var/lib/qarax/images")]
    image_cache_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let args = Args::parse();
    let addr = format!("0.0.0.0:{}", args.port).parse()?;

    info!("qarax-node starting on {}", addr);
    info!("Runtime directory: {}", args.runtime_dir.display());
    info!(
        "Cloud Hypervisor binary: {}",
        args.cloud_hypervisor_binary.display()
    );
    info!("virtiofsd binary: {}", args.virtiofsd_binary.display());
    info!("qarax-init binary: {}", args.qarax_init_binary.display());
    info!("Image cache dir: {}", args.image_cache_dir.display());

    // Ensure directories exist
    tokio::fs::create_dir_all(&args.runtime_dir).await?;
    tokio::fs::create_dir_all(&args.image_cache_dir).await?;

    // Build ImageStoreManager if virtiofsd is present
    let image_store_manager = if args.virtiofsd_binary.exists() {
        info!("virtiofsd found — OCI image boot enabled");
        Some(Arc::new(ImageStoreManager::new(
            &args.virtiofsd_binary,
            &args.qarax_init_binary,
            &args.image_cache_dir,
            &args.runtime_dir,
        )))
    } else {
        info!(
            "virtiofsd not found at {} — OCI image boot disabled",
            args.virtiofsd_binary.display()
        );
        None
    };

    // Build VmManager with optional ImageStoreManager
    let vm_manager = Arc::new(VmManager::new(
        &args.runtime_dir,
        &args.cloud_hypervisor_binary,
        image_store_manager,
    ));
    vm_manager.recover_vms().await;

    // Create the VM service from the manager
    let vm_service = VmServiceImpl::from_manager(vm_manager);

    info!("Starting gRPC server with Cloud Hypervisor backend");

    let file_transfer_service = FileTransferServiceImpl::new();

    // Start the gRPC server
    Server::builder()
        .add_service(VmServiceServer::new(vm_service))
        .add_service(FileTransferServiceServer::new(file_transfer_service))
        .serve(addr)
        .await?;

    Ok(())
}
