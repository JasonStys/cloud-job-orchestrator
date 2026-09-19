#![allow(clippy::unwrap_used)]
//! File: Property tests for graph validation and scheduler terminal-state invariants.
//! Functions: generators build chains and deliberate cycles across a wide input range.
//! Variables: generated sizes are bounded to keep the quality gate fast and deterministic.

use cloud_job_orchestrator::{DagSpec, Dependency, JobKind, JobSpec, Scheduler, SchedulerConfig};
use proptest::prelude::*;

fn jobs(count: usize) -> Vec<JobSpec> {
    (0..count)
        .map(|index| JobSpec {
            id: format!("job_{index}"),
            kind: JobKind::Checksum,
            payload: format!("payload-{index}"),
            max_attempts: 3,
            priority: i16::try_from(index % 10).unwrap(),
        })
        .collect()
}

proptest! {
    #[test]
    fn every_generated_chain_has_a_complete_topological_order(count in 1_usize..100) {
        let spec = DagSpec {
            id: "property_chain".to_owned(),
            jobs: jobs(count),
            dependencies: (1..count)
                .map(|index| Dependency { from: format!("job_{}", index - 1), to: format!("job_{index}") })
                .collect(),
        };
        let order = spec.validate().unwrap();
        prop_assert_eq!(order.len(), count);
        for index in 1..count {
            let parent = format!("job_{}", index - 1);
            let child = format!("job_{index}");
            let parent_position = order.iter().position(|id| id == &parent).unwrap();
            let child_position = order.iter().position(|id| id == &child).unwrap();
            prop_assert!(parent_position < child_position);
        }
    }

    #[test]
    fn generated_back_edge_is_rejected(count in 2_usize..100) {
        let mut dependencies: Vec<Dependency> = (1..count)
            .map(|index| Dependency { from: format!("job_{}", index - 1), to: format!("job_{index}") })
            .collect();
        dependencies.push(Dependency { from: format!("job_{}", count - 1), to: "job_0".to_owned() });
        let spec = DagSpec { id: "property_cycle".to_owned(), jobs: jobs(count), dependencies };
        prop_assert!(spec.validate().is_err());
    }
}

#[test]
fn every_job_reaches_a_terminal_state_in_a_successful_chain() {
    let count = 50;
    let spec = DagSpec {
        id: "terminal_chain".to_owned(),
        jobs: jobs(count),
        dependencies: (1..count)
            .map(|index| Dependency {
                from: format!("job_{}", index - 1),
                to: format!("job_{index}"),
            })
            .collect(),
    };
    let mut scheduler = Scheduler::new(SchedulerConfig::default());
    scheduler.submit(spec, 0).unwrap();
    for tick in 0..count {
        let claim = scheduler
            .claim("property-worker", i64::try_from(tick).unwrap(), 1_000)
            .unwrap()
            .unwrap();
        scheduler
            .complete(
                &claim.dag_id,
                &claim.job_id,
                "property-worker",
                &claim.lease_token,
                "ok".to_owned(),
                i64::try_from(tick).unwrap(),
            )
            .unwrap();
    }
    let snapshot = scheduler.snapshot("terminal_chain").unwrap();
    assert!(
        snapshot
            .jobs
            .iter()
            .all(|job| job.state == cloud_job_orchestrator::JobState::Succeeded)
    );
}
