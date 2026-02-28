use anyhow::Result;
use clap::{Parser, Subcommand};

mod api;
mod client;
mod commands;
mod console;

#[derive(Parser)]
#[command(name = "qarax", about = "CLI for the qarax VM management API", version)]
pub struct Cli {
    /// Server base URL
    #[arg(
        long,
        env = "QARAX_SERVER",
        default_value = "http://localhost:8000",
        global = true
    )]
    pub server: String,

    /// Print raw JSON instead of a formatted table
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Virtual machine operations
    Vm(commands::vm::VmArgs),
    /// Hypervisor host operations
    Host(commands::host::HostArgs),
    /// Storage pool operations
    StoragePool(commands::storage::StoragePoolArgs),
    /// Storage object operations
    StorageObject(commands::storage::StorageObjectArgs),
    /// File transfer operations
    Transfer(commands::transfer::TransferArgs),
    /// Boot source operations
    BootSource(commands::boot_source::BootSourceArgs),
    /// Network operations
    Network(commands::network::NetworkArgs),
    /// Async job operations
    Job(commands::job::JobArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = client::Client::new(&cli.server);

    match cli.command {
        Commands::Vm(args) => commands::vm::run(args, &client, cli.json).await,
        Commands::Host(args) => commands::host::run(args, &client, cli.json).await,
        Commands::StoragePool(args) => commands::storage::run_pool(args, &client, cli.json).await,
        Commands::StorageObject(args) => {
            commands::storage::run_object(args, &client, cli.json).await
        }
        Commands::Network(args) => commands::network::run(args, &client, cli.json).await,
        Commands::Transfer(args) => commands::transfer::run(args, &client, cli.json).await,
        Commands::BootSource(args) => commands::boot_source::run(args, &client, cli.json).await,
        Commands::Job(args) => commands::job::run(args, &client, cli.json).await,
    }
}
