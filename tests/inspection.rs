#![allow(clippy::unwrap_used)]
//! File: Independent structural/readiness and actual CLI oracles by Jason Stys.
//! Functions: fixtures define a shared dependency graph; tests cover blocked, retry and stale-lease evidence.
//! Variables: all clocks/states are explicit synthetic snapshots; no task or network execution.

use cloud_job_orchestrator::domain::JobSnapshot;
use cloud_job_orchestrator::inspection::{explain, plan};
use cloud_job_orchestrator::{DagSnapshot, DagSpec, Dependency, JobKind, JobSpec, JobState};

fn spec() -> DagSpec {
    DagSpec {
        id: "release".into(),
        jobs: ["publish", "test", "build", "review"]
            .into_iter()
            .map(|id| JobSpec {
                id: id.into(),
                kind: JobKind::Uppercase,
                payload: "fixture".into(),
                max_attempts: 3,
                priority: 0,
            })
            .collect(),
        dependencies: [
            ("build", "test"),
            ("build", "review"),
            ("test", "publish"),
            ("review", "publish"),
        ]
        .into_iter()
        .map(|(a, b)| Dependency {
            from: a.into(),
            to: b.into(),
        })
        .collect(),
    }
}

fn snapshot() -> DagSnapshot {
    DagSnapshot {
        id: "release".into(),
        jobs: spec()
            .jobs
            .iter()
            .map(|job| JobSnapshot {
                id: job.id.clone(),
                state: JobState::Pending,
                attempts: 0,
                max_attempts: 3,
                available_at_ms: 1100,
                lease_owner: None,
                lease_expires_at_ms: None,
                result: None,
            })
            .collect(),
    }
}

#[test]
fn dependency_waves_are_structural_stable_and_validate_before_planning() {
    let mut dag = spec();
    let before = serde_json::to_vec(&dag).unwrap();
    let result = plan(&dag).unwrap();
    assert_eq!(
        result
            .jobs
            .iter()
            .map(|row| (row.id.as_str(), row.dependency_wave))
            .collect::<Vec<_>>(),
        [("build", 0), ("review", 1), ("test", 1), ("publish", 2)]
    );
    assert_eq!(serde_json::to_vec(&dag).unwrap(), before);
    let original = serde_json::to_value(&result).unwrap();
    dag.dependencies.reverse();
    dag.jobs.reverse();
    assert_eq!(serde_json::to_value(plan(&dag).unwrap()).unwrap(), original);
    dag.dependencies.push(dag.dependencies[0].clone());
    assert!(
        plan(&dag).is_err(),
        "duplicate edges rejected before PostgreSQL constraint"
    );
    dag.dependencies.pop();
    dag.dependencies.push(Dependency {
        from: "publish".into(),
        to: "build".into(),
    });
    assert!(plan(&dag).is_err());
}

#[test]
fn terminal_ancestors_and_unmet_direct_parents_are_distinct() {
    let mut state = snapshot();
    state
        .jobs
        .iter_mut()
        .find(|job| job.id == "build")
        .unwrap()
        .state = JobState::DeadLettered;
    let before = serde_json::to_vec(&state).unwrap();
    let result = explain(&state, &spec().dependencies, 1000).unwrap();
    let publish = result.iter().find(|row| row.id == "publish").unwrap();
    assert_eq!(publish.unmet_parents, ["review", "test"]);
    assert_eq!(publish.terminal_blockers, ["build"]);
    assert_eq!(
        serde_json::to_vec(&state).unwrap(),
        before,
        "explanation is read-only"
    );
    for job in &mut state.jobs {
        if job.id == "review" || job.id == "test" {
            job.state = JobState::Succeeded;
        }
    }
    let result = explain(&state, &spec().dependencies, 1000).unwrap();
    assert!(result[0].unmet_parents.is_empty());
    assert!(
        result[0].terminal_blockers.is_empty(),
        "satisfied edges must not resurrect older ancestors"
    );
    assert_ne!(
        serde_json::to_vec(&state).unwrap(),
        before,
        "fixture changed explicitly, helper did not"
    );
}

#[test]
fn countdowns_are_snapshot_bound_exact_inclusive_and_overflow_safe() {
    let mut state = snapshot();
    state.jobs[0].state = JobState::RetryWait;
    state.jobs[1].state = JobState::Leased;
    state.jobs[1].lease_expires_at_ms = Some(1000);
    let before = serde_json::to_vec(&state).unwrap();
    let result = explain(&state, &spec().dependencies, 1000).unwrap();
    assert_eq!(result[0].retry_wait_remaining_ms, Some(100));
    assert_eq!(result[1].lease_remaining_ms, Some(0));
    assert!(result[1].lease_expired_at_snapshot);
    assert!(result[2].lease_remaining_ms.is_none());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    state.jobs[0].available_at_ms = i64::MAX;
    assert_eq!(
        explain(&state, &[], i64::MIN).unwrap()[0].retry_wait_remaining_ms,
        Some(i64::MAX)
    );
}

#[test]
fn corrupt_unknown_duplicate_or_oversized_snapshots_fail() {
    let mut state = snapshot();
    assert!(
        explain(
            &state,
            &[Dependency {
                from: "absent".into(),
                to: "publish".into()
            }],
            0
        )
        .is_err()
    );
    state.jobs.push(state.jobs[0].clone());
    assert!(explain(&state, &[], 0).is_err());
    state.jobs.clear();
    assert!(explain(&state, &[], 0).is_err());
    state.jobs = vec![snapshot().jobs[0].clone(); 1001];
    assert!(explain(&state, &[], 0).is_err());
}

#[test]
fn strict_json_and_actual_cli_plan_require_no_database_or_execution() {
    let mut value = serde_json::to_value(spec()).unwrap();
    value["shell"] = serde_json::json!("never execute");
    assert!(serde_json::from_value::<DagSpec>(value).is_err());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_orchestratorctl"))
        .args(["plan", "examples/release-dag.json"])
        .env_remove("DATABASE_URL")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["jobs"].as_array().unwrap().len() >= 3);
    assert!(
        value["notice"]
            .as_str()
            .unwrap()
            .contains("No DAG was submitted")
    );
}

#[test]
fn maximum_cancelled_and_partially_failed_chains_have_explicit_bounded_reports() {
    let mut state = snapshot();
    state.jobs = (0..1000)
        .map(|index| {
            let mut job = snapshot().jobs[0].clone();
            job.id = format!("node_{index:04}_{}", "x".repeat(54));
            job.state = JobState::Cancelled;
            job
        })
        .collect();
    let edges: Vec<_> = state
        .jobs
        .windows(2)
        .map(|pair| Dependency {
            from: pair[0].id.clone(),
            to: pair[1].id.clone(),
        })
        .collect();
    let rows = explain(&state, &edges, 0).unwrap();
    assert!(rows.iter().all(|row| row.terminal_blocker_count == 0));
    for job in &mut state.jobs[500..] {
        job.state = JobState::Pending;
    }
    let rows = explain(&state, &edges, 0).unwrap();
    assert!(
        rows[500..]
            .iter()
            .all(|row| row.terminal_blocker_count == 500 && row.terminal_blockers_truncated)
    );
    assert!(rows.iter().all(|row| row.terminal_blockers.len() <= 16));
    assert_eq!(
        rows.iter()
            .map(|row| row.terminal_blockers.len())
            .sum::<usize>(),
        1000
    );
    assert!(
        serde_json::to_vec(&rows).unwrap().len() < 1_000_000,
        "bounded report must not amplify a valid graph into tens of MB"
    );
}
