//! File: Runs a durable worker loop for the fixed synthetic operation allowlist.
//! Function: `main` claims, executes, and commits jobs while respecting lease ownership.
//! Variables: database URL, worker ID, lease duration, and idle poll interval come from environment variables.

use cloud_job_orchestrator::{PostgresStore, worker};
use std::{env, time::Duration};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().json().init();
    let database_url = env::var("DATABASE_URL")?;
    let worker_id = env::var("WORKER_ID").unwrap_or_else(|_| format!("worker-{}", Uuid::new_v4()));
    let lease_ms = parse_bounded("LEASE_MS", 15_000, 1, 60_000)?;
    let poll_ms = parse_bounded("POLL_MS", 500, 10, 10_000)?;
    let store = PostgresStore::connect(&database_url, 5).await?;
    store.migrate().await?;
    loop {
        if let Some(claim) = store.claim(&worker_id, lease_ms).await? {
            let token = Uuid::parse_str(&claim.lease_token)?;
            match worker::execute(&claim.kind, &claim.payload).await {
                Ok(result) => {
                    store
                        .complete(&claim.dag_id, &claim.job_id, &worker_id, token, &result)
                        .await?;
                }
                Err(error) => {
                    tracing::warn!(dag_id = %claim.dag_id, job_id = %claim.job_id, error = %error, "job failed");
                    store
                        .fail(&claim.dag_id, &claim.job_id, &worker_id, token)
                        .await?;
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(u64::try_from(poll_ms)?)).await;
        }
    }
}

fn parse_bounded(
    name: &str,
    default: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, Box<dyn std::error::Error>> {
    let value = env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse::<i64>()?;
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(format!("{name} must be between {minimum} and {maximum}").into())
    }
}
