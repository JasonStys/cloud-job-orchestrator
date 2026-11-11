//! File: Starts the native application's owned actual PostgreSQL/API/worker composition.
//! Functions: run keeps the profile lock through database close; main exposes only fixed public errors.
//! Variables: paths are explicit private/profile or immutable-resource roots, never a substitute store.

use clap::Parser;
use cloud_job_orchestrator::{
    native_database::NativeDatabase,
    native_profile::{Profile, program_roots},
    native_runtime,
};
use std::{io, path::PathBuf};

#[derive(Parser)]
struct Arguments {
    #[arg(long)]
    profile: PathBuf,
    #[arg(long)]
    postgres: PathBuf,
    #[arg(long)]
    assets: PathBuf,
}

async fn run(arguments: Arguments) -> io::Result<()> {
    if !cfg!(windows) {
        return Err(io::Error::other(
            "The current bundled native distribution targets Windows x64",
        ));
    }
    let mut input = tokio::io::stdin();
    let capability = native_runtime::capability(&mut input).await?;
    let executable = std::env::current_exe()?;
    let program = executable
        .parent()
        .ok_or_else(|| io::Error::other("Native program root is missing"))?;
    let profile = Profile::open(
        &arguments.profile,
        &program_roots(program, &arguments.assets, &arguments.postgres)?,
    )?;
    std::env::set_current_dir(&arguments.assets)?;
    let database = NativeDatabase::start(&profile, &arguments.postgres).await?;
    eprintln!("Owned native stage: PostgreSQL started");
    let result = if let Ok(store) = database.connect(&profile).await {
        eprintln!("Owned native stage: PostgreSQL identity verified");
        if store.migrate().await.is_err() {
            store.close().await;
            Err(io::Error::other(
                "Native database migration failed; no reset attempted",
            ))
        } else {
            eprintln!("Owned native stage: migrations ready");
            let result = native_runtime::serve(store.clone(), &capability, input).await;
            store.close().await;
            result
        }
    } else {
        eprintln!("Owned native stage: PostgreSQL identity verification failed");
        Err(io::Error::other(
            "The exact native PostgreSQL identity could not be verified",
        ))
    };
    let shutdown = database.close().await;
    drop(profile);
    result.and(shutdown)
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if run(Arguments::parse()).await.is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        eprintln!(
            "Owned native startup/work/shutdown failed. Preserve the profile and inspect its local PostgreSQL startup log; no reset was attempted."
        );
        std::process::ExitCode::FAILURE
    }
}
