use clap::{Args, Subcommand};
use uuid::Uuid;

use crate::{api, client::Client};

use super::{OutputFormat, print_output};

#[derive(Args)]
pub struct JobArgs {
    #[command(subcommand)]
    command: JobCommand,
}

#[derive(Subcommand)]
enum JobCommand {
    /// Get details of an async job
    Get {
        /// Job ID
        id: Uuid,
    },
}

pub async fn run(args: JobArgs, client: &Client, output: OutputFormat) -> anyhow::Result<()> {
    match args.command {
        JobCommand::Get { id } => {
            let job = api::jobs::get(client, id).await?;
            if !matches!(output, OutputFormat::Table) {
                print_output(&job, output)?;
            } else {
                println!("ID:          {}", job.id);
                println!("Type:        {}", job.job_type);
                println!("Status:      {}", job.status);
                println!("Progress:    {}%", job.progress.unwrap_or(0));
                if let Some(desc) = &job.description {
                    println!("Description: {desc}");
                }
                if let Some(res) = &job.resource_id {
                    println!("Resource:    {res}");
                }
                if let Some(err) = &job.error {
                    println!("Error:       {err}");
                }
                println!("Created:     {}", job.created_at);
                println!("Updated:     {}", job.updated_at);
            }
        }
    }

    Ok(())
}
