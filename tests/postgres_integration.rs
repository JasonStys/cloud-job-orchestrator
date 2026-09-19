#![allow(clippy::unwrap_used)]
//! File: PostgreSQL integration tests for migrations, concurrent claims, lease expiry, and CAS completion.
//! Functions: tests use unique DAG identifiers and skip only when `DATABASE_URL` is intentionally absent.
//! Variables: each test owns a unique identifier suffix so parallel runs cannot collide.

use cloud_job_orchestrator::{DagSpec, JobKind, JobSpec, JobState, PostgresStore, StoreError};
use std::collections::HashSet;
use uuid::Uuid;

async fn store() -> Option<PostgresStore> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let store = PostgresStore::connect(&url, 20).await.unwrap();
    store.migrate().await.unwrap();
    Some(store)
}

fn dag_with_jobs(id: String, count: usize, max_attempts: u16) -> DagSpec {
    DagSpec {
        id,
        jobs: (0..count)
            .map(|index| JobSpec {
                id: format!("job_{index}"),
                kind: JobKind::Uppercase,
                payload: "bounded".to_owned(),
                max_attempts,
                priority: 0,
            })
            .collect(),
        dependencies: vec![],
    }
}

#[tokio::test]
async fn concurrent_workers_never_receive_the_same_claim() {
    let Some(store) = store().await else { return };
    let dag_id = format!("concurrency_{}", Uuid::new_v4().simple());
    store.submit(&dag_with_jobs(dag_id, 16, 3)).await.unwrap();
    let mut handles = Vec::new();
    for worker in 0..16 {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            store
                .claim(&format!("worker-{worker}"), 5_000)
                .await
                .unwrap()
                .unwrap()
        }));
    }
    let mut claimed = HashSet::new();
    for handle in handles {
        claimed.insert(handle.await.unwrap().job_id);
    }
    assert_eq!(claimed.len(), 16);
}

#[tokio::test]
async fn expiry_requeues_and_rejects_a_late_worker() {
    let Some(store) = store().await else { return };
    let dag_id = format!("expiry_{}", Uuid::new_v4().simple());
    store
        .submit(&dag_with_jobs(dag_id.clone(), 1, 3))
        .await
        .unwrap();
    let claim = store.claim("worker-a", 20).await.unwrap().unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert_eq!(store.reap().await.unwrap(), 1);
    let late = store
        .complete(
            &dag_id,
            &claim.job_id,
            "worker-a",
            Uuid::parse_str(&claim.lease_token).unwrap(),
            "late",
        )
        .await;
    assert!(matches!(late, Err(StoreError::LeaseConflict)));
    assert_eq!(
        store.snapshot(&dag_id).await.unwrap().jobs[0].state,
        JobState::RetryWait
    );
}
