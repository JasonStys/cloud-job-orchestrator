#![allow(clippy::unwrap_used)]
//! File: PostgreSQL integration tests for migrations, concurrent claims, lease expiry, and CAS completion.
//! Functions: explicitly ignored tests require disposable loopback PostgreSQL; CI opts in and absence fails.
//! Variables: each test owns a unique identifier suffix so parallel runs cannot collide.

use cloud_job_orchestrator::database as sqlx;
use cloud_job_orchestrator::{
    DagSpec, Dependency, JobKind, JobSpec, JobState, PostgresStore, StoreError,
};
use std::collections::HashSet;
use uuid::Uuid;

async fn store() -> PostgresStore {
    let url = std::env::var("DATABASE_URL").unwrap();
    let options: sqlx::postgres::PgConnectOptions = url.parse().unwrap();
    assert!(
        ["127.0.0.1", "localhost"].contains(&options.get_host()),
        "disposable loopback database required"
    );
    assert_eq!(
        options.get_database(),
        Some("orchestrator_test"),
        "dedicated test database required"
    );
    let store = PostgresStore::connect(&url, 20).await.unwrap();
    store.migrate().await.unwrap();
    store
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
#[ignore = "requires disposable loopback orchestrator_test PostgreSQL; CI runs explicitly"]
async fn concurrent_workers_never_receive_the_same_claim() {
    let store = store().await;
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
#[ignore = "requires disposable loopback orchestrator_test PostgreSQL; CI runs explicitly"]
async fn expiry_requeues_and_rejects_a_late_worker() {
    let store = store().await;
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

#[tokio::test]
#[ignore = "requires disposable loopback orchestrator_test PostgreSQL; CI runs explicitly"]
async fn captured_inspection_explains_blockers_and_omits_application_results() {
    let store = store().await;
    let id = format!("inspect_{}", Uuid::new_v4().simple());
    let mut dag = dag_with_jobs(id.clone(), 3, 1);
    dag.dependencies = vec![
        Dependency {
            from: "job_0".into(),
            to: "job_1".into(),
        },
        Dependency {
            from: "job_1".into(),
            to: "job_2".into(),
        },
    ];
    store.submit(&dag).await.unwrap();
    let before = store.snapshot(&id).await.unwrap();
    let inspection = store.inspection(&id).await.unwrap();
    assert_eq!(inspection.explanations[1].unmet_parents, ["job_0"]);
    assert!(inspection.events.is_empty());
    assert_eq!(
        before,
        store.snapshot(&id).await.unwrap(),
        "inspection does not claim/reap work"
    );
    let claim = store
        .claim("inspection-worker", 5000)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claim.dag_id, id);
    store
        .fail(
            &id,
            &claim.job_id,
            "inspection-worker",
            Uuid::parse_str(&claim.lease_token).unwrap(),
        )
        .await
        .unwrap();
    let inspection = store.inspection(&id).await.unwrap();
    assert_eq!(inspection.explanations[2].terminal_blockers, ["job_0"]);
    assert_eq!(inspection.events[0].event_type, "dead_lettered");
    assert!(
        inspection
            .events
            .windows(2)
            .all(|pair| pair[0].sequence.parse::<i64>().unwrap()
                > pair[1].sequence.parse::<i64>().unwrap())
    );
    assert!(!inspection.events_truncated);
    assert!(matches!(
        store.inspection("absent_workflow").await,
        Err(StoreError::NotFound)
    ));
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    // Authored event-window fixture, separate from actual worker evidence in verify-live.mjs.
    sqlx::query("INSERT INTO job_events (dag_id,job_id,event_type,attempt) SELECT $1,'job_0','fixture_window',1 FROM generate_series(1,201)")
        .bind(&id).execute(&pool).await.unwrap();
    let window = store.inspection(&id).await.unwrap();
    assert_eq!(window.events.len(), 200);
    assert!(window.events_truncated);
    assert!(
        window
            .events
            .iter()
            .all(|event| event.event_type == "fixture_window")
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires disposable loopback orchestrator_test PostgreSQL; CI runs explicitly"]
async fn captured_job_states_and_events_remain_coherent_during_actual_worker_transactions() {
    let store = store().await;
    let id = format!("coherent_{}", Uuid::new_v4().simple());
    store
        .submit(&dag_with_jobs(id.clone(), 32, 1))
        .await
        .unwrap();
    let worker = store.clone();
    let worker_id = id.clone();
    let writer = tokio::spawn(async move {
        for _ in 0..32 {
            let claim = worker
                .claim("coherence-worker", 5000)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(claim.dag_id, worker_id);
            worker
                .complete(
                    &worker_id,
                    &claim.job_id,
                    "coherence-worker",
                    Uuid::parse_str(&claim.lease_token).unwrap(),
                    "synthetic-result",
                )
                .await
                .unwrap();
            tokio::task::yield_now().await;
        }
    });
    for _ in 0..32 {
        let captured = store.inspection(&id).await.unwrap();
        assert!(!captured.events_truncated);
        for job in &captured.snapshot.jobs {
            let events: Vec<_> = captured
                .events
                .iter()
                .filter(|event| event.job_id == job.id)
                .collect();
            match &job.state {
                JobState::Ready => {
                    assert_eq!(job.attempts, 0);
                    assert!(events.is_empty());
                }
                JobState::Leased => {
                    assert_eq!(job.attempts, 1);
                    assert_eq!(events.len(), 1);
                    assert_eq!(events[0].event_type, "claimed");
                }
                JobState::Succeeded => {
                    assert_eq!(job.attempts, 1);
                    assert_eq!(events.len(), 2);
                    assert_eq!(events[0].event_type, "succeeded");
                }
                unexpected => assert_eq!(
                    unexpected,
                    &JobState::Succeeded,
                    "unexpected authored acceptance state"
                ),
            }
            assert!(job.result.is_none());
        }
    }
    writer.await.unwrap();
    assert!(
        store
            .inspection(&id)
            .await
            .unwrap()
            .snapshot
            .jobs
            .iter()
            .all(|job| job.state == JobState::Succeeded)
    );
}
