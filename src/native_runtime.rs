//! File: Supervises real PostgreSQL workers and HTTP delivery through completion-aware native stop.
//! Functions: serve binds port0, records public readiness and drains retained tasks; work keeps CAS.
//! Variables: capability stays in process, pause is requested state, active is an observation counter.

use crate::{
    PostgresStore, StoreError,
    http::AppState,
    native_http::{self, Boundary},
    native_profile, worker,
};
use serde_json::json;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    sync::watch,
    task::JoinSet,
};
use uuid::Uuid;

/// Read one fixed capability line before startup; never accept unlimited stdin or command text.
pub async fn capability(input: &mut (impl AsyncRead + Unpin)) -> io::Result<String> {
    let mut bytes = Vec::with_capacity(65);
    while bytes.len() <= 64 {
        let byte = input.read_u8().await?;
        if byte == b'\n' {
            break;
        }
        bytes.push(byte);
    }
    if bytes.len() != 64
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Expected the exact native owner capability",
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid native capability encoding",
        )
    })
}

/// Serve the actual API and two retained workers; parent EOF requests a normal complete drain.
pub async fn serve(
    store: PostgresStore,
    secret: &str,
    mut input: impl AsyncRead + Unpin,
) -> io::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let (stopping, stop) = watch::channel(false);
    let (paused, pause) = watch::channel(true);
    let active = Arc::new(AtomicUsize::new(0));
    let instance = native_profile::nonce();
    let boundary = Boundary::new(address, secret, &instance, paused, Arc::clone(&active))
        .map_err(io::Error::other)?;
    let router = native_http::router(
        AppState {
            store: store.clone(),
        },
        boundary,
    );
    let mut tasks = JoinSet::new();
    for index in 0..2 {
        tasks.spawn(work(
            store.clone(),
            format!("native-worker-{index}"),
            stop.clone(),
            pause.clone(),
            Arc::clone(&active),
        ));
    }
    let mut server_stop = stop;
    let mut server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                while !*server_stop.borrow() && server_stop.changed().await.is_ok() {}
            })
            .await
    });
    let ready = json!({"product":"cloud-job-orchestrator","version":env!("CARGO_PKG_VERSION"),
        "source":option_env!("CJO_BUILD_SOURCE"),"source_dirty":option_env!("CJO_BUILD_DIRTY") != Some("false"),
        "port":address.port(),"instance":instance});
    let mut output = tokio::io::stdout();
    let rendered = serde_json::to_vec(&ready).map_err(io::Error::other)?;
    let announced = async {
        output.write_all(&rendered).await?;
        output.write_all(b"\n").await?;
        output.flush().await
    }
    .await;
    let mut server_finished = false;
    let result = if let Err(error) = announced {
        Err(error)
    } else {
        tokio::select! {
            _ = input.read_u8() => Ok(()),
            result = &mut server => {
                server_finished = true;
                match result {
                    Ok(Err(error)) => Err(error),
                    Ok(Ok(())) => Err(io::Error::other("Owned HTTP server closed unexpectedly")),
                    Err(error) => Err(io::Error::other(error)),
                }
            },
            _ = tasks.join_next() => Err(io::Error::other("An owned native worker closed unexpectedly")),
        }
    };
    stopping.send_replace(true);
    let mut failed = result.is_err();
    while let Some(task) = tasks.join_next().await {
        failed |= !matches!(task, Ok(Ok(())));
    }
    if !server_finished {
        failed |= !matches!(server.await, Ok(Ok(())));
    }
    store.close().await;
    if failed {
        Err(io::Error::other(
            "Owned service work did not complete normally; retained state is preserved",
        ))
    } else {
        Ok(())
    }
}

async fn work(
    store: PostgresStore,
    owner: String,
    mut stop: watch::Receiver<bool>,
    mut pause: watch::Receiver<bool>,
    active: Arc<AtomicUsize>,
) -> Result<(), StoreError> {
    loop {
        if *stop.borrow() {
            return Ok(());
        }
        if *pause.borrow() {
            tokio::select! { _ = stop.changed() => {}, _ = pause.changed() => {} }
            continue;
        }
        if let Some(claim) = store.claim(&owner, 15_000).await? {
            let token = Uuid::parse_str(&claim.lease_token).map_err(|_| StoreError::CorruptData)?;
            active.fetch_add(1, Ordering::SeqCst);
            let result = match worker::execute(&claim.kind, &claim.payload).await {
                Ok(value) => {
                    store
                        .complete(&claim.dag_id, &claim.job_id, &owner, token, &value)
                        .await
                }
                Err(_) => store
                    .fail(&claim.dag_id, &claim.job_id, &owner, token)
                    .await
                    .map(|_| ()),
            };
            active.fetch_sub(1, Ordering::SeqCst);
            // Cancellation/expiry may legitimately revoke the CAS while harmless work was in flight.
            if !matches!(result, Ok(()) | Err(StoreError::LeaseConflict)) {
                result?;
            }
        } else {
            tokio::select! { _ = stop.changed() => {}, () = tokio::time::sleep(Duration::from_millis(100)) => {} }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::capability;
    #[tokio::test]
    async fn only_exact_bounded_lowercase_owner_material_is_admitted() {
        for input in [
            "a".repeat(64) + "\n",
            "\n".into(),
            "A".repeat(64) + "\n",
            "a".repeat(65) + "\n",
        ] {
            let valid = input == "a".repeat(64) + "\n";
            assert_eq!(capability(&mut input.as_bytes()).await.is_ok(), valid);
        }
    }
}
