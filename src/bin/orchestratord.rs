//! File: Starts the HTTP control plane after migrations and readiness initialization.
//! Function: `main` loads bounded environment configuration, initializes JSON tracing, and serves Axum.
//! Variables: `DATABASE_URL`, `BIND_ADDRESS`, and `DATABASE_MAX_CONNECTIONS` configure the process.

use cloud_job_orchestrator::{
    PostgresStore,
    http::{AppState, router},
};
use std::{env, net::SocketAddr};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let database_url = env::var("DATABASE_URL")?;
    let bind: SocketAddr = env::var("BIND_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
        .parse()?;
    let max_connections = env::var("DATABASE_MAX_CONNECTIONS")
        .unwrap_or_else(|_| "10".to_owned())
        .parse()?;
    let store = PostgresStore::connect(&database_url, max_connections).await?;
    store.migrate().await?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(address = %bind, "control plane is listening");
    axum::serve(listener, router(AppState { store })).await?;
    Ok(())
}
