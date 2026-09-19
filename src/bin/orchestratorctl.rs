//! File: Provides validation, submission, inspection, cancellation, and lease-reaping commands.
//! Functions: `main` dispatches typed CLI commands; `read_spec` parses a bounded local JSON DAG.
//! Variables: subcommands receive a database URL explicitly or through `DATABASE_URL`.

use clap::{Parser, Subcommand};
use cloud_job_orchestrator::{DagSpec, PostgresStore};
use std::{fs, path::PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "orchestratorctl",
    version,
    about = "Operate the safe DAG control plane"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate {
        file: PathBuf,
    },
    Submit {
        file: PathBuf,
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
    },
    Status {
        dag_id: String,
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
    },
    Cancel {
        dag_id: String,
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
    },
    Reap {
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Validate { file } => {
            let spec = read_spec(&file)?;
            let order = spec.validate()?;
            println!("valid DAG {}: {}", spec.id, order.join(" -> "));
        }
        Command::Submit { file, database_url } => {
            let spec = read_spec(&file)?;
            let store = connect(&database_url).await?;
            store.submit(&spec).await?;
            println!("submitted {}", spec.id);
        }
        Command::Status {
            dag_id,
            database_url,
        } => {
            let snapshot = connect(&database_url).await?.snapshot(&dag_id).await?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        Command::Cancel {
            dag_id,
            database_url,
        } => {
            let cancelled = connect(&database_url).await?.cancel(&dag_id).await?;
            println!("cancelled {cancelled} jobs in {dag_id}");
        }
        Command::Reap { database_url } => {
            let reclaimed = connect(&database_url).await?.reap().await?;
            println!("reclaimed {reclaimed} expired leases");
        }
    }
    Ok(())
}

fn read_spec(path: &PathBuf) -> Result<DagSpec, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > 1_000_000 {
        return Err("DAG input exceeds 1 MB".into());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

async fn connect(database_url: &str) -> Result<PostgresStore, Box<dyn std::error::Error>> {
    let store = PostgresStore::connect(database_url, 5).await?;
    store.migrate().await?;
    Ok(store)
}
