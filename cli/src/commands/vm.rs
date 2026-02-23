use anyhow::anyhow;
use clap::{Args, Subcommand};
use tabled::{Table, Tabled};
use uuid::Uuid;

use crate::{
    api::{
        self,
        models::{CreateVmResult, NewVm},
    },
    client::Client,
    console,
};

use super::{format_bytes, print_json, resolve_boot_source_id, resolve_vm_id};

#[derive(Args)]
pub struct VmArgs {
    #[command(subcommand)]
    command: VmCommand,
}

#[derive(Subcommand)]
enum VmCommand {
    /// List all VMs
    List,
    /// Get details of a specific VM
    Get {
        /// VM name or ID
        vm: String,
    },
    /// Create a new VM
    Create {
        /// VM name
        #[arg(long)]
        name: String,
        /// Number of vCPUs at boot
        #[arg(long)]
        vcpus: i32,
        /// Maximum vCPUs (defaults to --vcpus)
        #[arg(long)]
        max_vcpus: Option<i32>,
        /// Memory size in bytes
        #[arg(long)]
        memory: i64,
        /// Boot source name or ID
        #[arg(long)]
        boot_source: Option<String>,
        /// Description
        #[arg(long)]
        description: Option<String>,
        /// OCI image reference (triggers async creation)
        #[arg(long)]
        image_ref: Option<String>,
    },
    /// Delete a VM
    Delete {
        /// VM name or ID
        vm: String,
    },
    /// Start a VM
    Start {
        /// VM name or ID
        vm: String,
    },
    /// Stop a VM
    Stop {
        /// VM name or ID
        vm: String,
    },
    /// Pause a VM
    Pause {
        /// VM name or ID
        vm: String,
    },
    /// Resume a paused VM
    Resume {
        /// VM name or ID
        vm: String,
    },
    /// Print the VM's console log
    Console {
        /// VM name or ID
        vm: String,
    },
    /// Attach an interactive WebSocket console to a running VM
    Attach {
        /// VM name or ID
        vm: String,
    },
}

#[derive(Tabled)]
struct VmRow {
    #[tabled(rename = "ID")]
    id: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Host")]
    host_id: String,
    #[tabled(rename = "vCPUs")]
    vcpus: String,
    #[tabled(rename = "Memory")]
    memory: String,
    #[tabled(rename = "Image")]
    image_ref: String,
}

pub async fn run(args: VmArgs, client: &Client, json: bool) -> anyhow::Result<()> {
    match args.command {
        VmCommand::List => {
            let vms = api::vms::list(client).await?;
            if json {
                print_json(&vms)?;
            } else {
                let rows: Vec<VmRow> = vms
                    .iter()
                    .map(|vm| VmRow {
                        id: vm.id.to_string(),
                        name: vm.name.clone(),
                        status: vm.status.clone(),
                        host_id: vm
                            .host_id
                            .map(|h| h.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                        vcpus: format!("{}/{}", vm.boot_vcpus, vm.max_vcpus),
                        memory: format_bytes(vm.memory_size),
                        image_ref: vm.image_ref.clone().unwrap_or_else(|| "-".to_string()),
                    })
                    .collect();
                println!("{}", Table::new(rows));
            }
        }

        VmCommand::Get { vm } => {
            let id = resolve_vm_id(client, &vm).await?;
            let vm = api::vms::get(client, id).await?;
            if json {
                print_json(&vm)?;
            } else {
                println!("ID:          {}", vm.id);
                println!("Name:        {}", vm.name);
                println!("Status:      {}", vm.status);
                println!("Hypervisor:  {}", vm.hypervisor);
                println!(
                    "Host:        {}",
                    vm.host_id
                        .map(|h| h.to_string())
                        .unwrap_or_else(|| "-".to_string())
                );
                println!("vCPUs:       {}/{}", vm.boot_vcpus, vm.max_vcpus);
                println!("Memory:      {}", format_bytes(vm.memory_size));
                if let Some(bs) = vm.boot_source_id {
                    println!("Boot source: {bs}");
                }
                if let Some(desc) = &vm.description {
                    println!("Description: {desc}");
                }
                if let Some(img) = &vm.image_ref {
                    println!("Image:       {img}");
                }
            }
        }

        VmCommand::Create {
            name,
            vcpus,
            max_vcpus,
            memory,
            boot_source,
            description,
            image_ref,
        } => {
            let boot_source_id = match boot_source {
                Some(ref s) => Some(resolve_boot_source_id(client, s).await?),
                None => None,
            };
            let new_vm = NewVm {
                name,
                hypervisor: "cloud_hv".to_string(),
                boot_vcpus: vcpus,
                max_vcpus: max_vcpus.unwrap_or(vcpus),
                memory_size: memory,
                boot_source_id,
                description,
                image_ref: image_ref.clone(),
                config: serde_json::json!({}),
            };

            let result = api::vms::create(client, &new_vm).await?;
            match result {
                CreateVmResult::Created(vm_id) => {
                    if json {
                        print_json(&serde_json::json!({ "vm_id": vm_id }))?;
                    } else {
                        println!("Created VM: {vm_id}");
                    }
                }
                CreateVmResult::Accepted { vm_id, job_id } => {
                    if json {
                        print_json(&serde_json::json!({ "vm_id": vm_id, "job_id": job_id }))?;
                    } else {
                        println!("Creating VM: {vm_id}");
                        println!("Job:         {job_id}");
                        poll_job(client, job_id).await?;
                    }
                }
            }
        }

        VmCommand::Delete { vm } => {
            let id = resolve_vm_id(client, &vm).await?;
            api::vms::delete(client, id).await?;
            println!("Deleted VM: {id}");
        }

        VmCommand::Start { vm } => {
            let id = resolve_vm_id(client, &vm).await?;
            api::vms::start(client, id).await?;
            println!("Starting VM: {id}");
        }

        VmCommand::Stop { vm } => {
            let id = resolve_vm_id(client, &vm).await?;
            api::vms::stop(client, id).await?;
            println!("Stopped VM: {id}");
        }

        VmCommand::Pause { vm } => {
            let id = resolve_vm_id(client, &vm).await?;
            api::vms::pause(client, id).await?;
            println!("Paused VM: {id}");
        }

        VmCommand::Resume { vm } => {
            let id = resolve_vm_id(client, &vm).await?;
            api::vms::resume(client, id).await?;
            println!("Resumed VM: {id}");
        }

        VmCommand::Console { vm } => {
            let id = resolve_vm_id(client, &vm).await?;
            let log = api::vms::console_log(client, id).await?;
            print!("{log}");
        }

        VmCommand::Attach { vm } => {
            let id = resolve_vm_id(client, &vm).await?;
            console::attach(client.base_url(), id).await?;
        }
    }

    Ok(())
}

/// Poll a job until it completes or fails, printing progress to stderr.
async fn poll_job(client: &Client, job_id: Uuid) -> anyhow::Result<()> {
    use std::io::Write as _;
    loop {
        let job = api::jobs::get(client, job_id).await?;
        match job.status.as_str() {
            "completed" => {
                eprintln!("\r[completed]                    ");
                return Ok(());
            }
            "failed" => {
                return Err(anyhow!(
                    "Job {job_id} failed: {}",
                    job.error.unwrap_or_else(|| "unknown error".to_string())
                ));
            }
            status => {
                eprint!("\r[{status}] {}%   ", job.progress.unwrap_or(0));
                let _ = std::io::stderr().flush();
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}
