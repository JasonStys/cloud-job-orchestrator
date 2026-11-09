//! File: Read-only structural DAG plans and captured-state dependency explanations by Jason Stys.
//! Functions: `plan` validates without submission; `explain` preserves blocked/expired states.
//! Variables: sorted maps provide deterministic rows; all inputs retain existing graph bounds.

use crate::domain::{DagSnapshot, DagSpec, Dependency, JobState, ValidationError};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const MAX_BLOCKER_IDS_PER_JOB: usize = 16;
const MAX_BLOCKER_IDS_PER_REPORT: usize = 1000;

/// Structural parallel waves, not a runtime schedule or duration prediction.
#[derive(Debug, Serialize)]
pub struct GraphPlan {
    pub dag_id: String,
    pub jobs: Vec<PlannedJob>,
    pub notice: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PlannedJob {
    pub id: String,
    pub parents: Vec<String>,
    pub dependency_wave: usize,
}

/// Validate the entire graph before deriving earliest dependency-only waves.
pub fn plan(spec: &DagSpec) -> Result<GraphPlan, ValidationError> {
    let order = spec.validate()?;
    let parents = parent_map(&spec.dependencies);
    let mut waves = BTreeMap::new();
    let mut jobs = Vec::new();
    for id in order {
        let dependencies = parents.get(id.as_str()).cloned().unwrap_or_default();
        let wave = dependencies
            .iter()
            .filter_map(|parent| waves.get(*parent))
            .max()
            .map_or(0, |wave| wave + 1);
        waves.insert(id.clone(), wave);
        jobs.push(PlannedJob {
            id,
            parents: dependencies.into_iter().map(str::to_owned).collect(),
            dependency_wave: wave,
        });
    }
    Ok(GraphPlan {
        dag_id: spec.id.clone(),
        jobs,
        notice: "READ-ONLY structural plan. Waves show dependency depth only, not runtime concurrency, deadlines, fairness or duration. No DAG was submitted and no task executed.",
    })
}

fn parent_map(edges: &[Dependency]) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut parents: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for edge in edges {
        parents.entry(&edge.to).or_default().insert(&edge.from);
    }
    parents
}

#[derive(Debug, Serialize)]
pub struct JobExplanation {
    pub id: String,
    pub unmet_parents: Vec<String>,
    pub terminal_blockers: Vec<String>,
    pub terminal_blocker_count: usize,
    pub terminal_blockers_truncated: bool,
    pub retry_wait_remaining_ms: Option<i64>,
    pub lease_remaining_ms: Option<i64>,
    pub lease_expired_at_snapshot: bool,
}

/// Explain the captured state without mutating leases, reaping work or guessing why an event is missing.
pub fn explain(
    snapshot: &DagSnapshot,
    edges: &[Dependency],
    observed_at_ms: i64,
) -> Result<Vec<JobExplanation>, ValidationError> {
    if snapshot.jobs.is_empty()
        || snapshot.jobs.len() > crate::domain::MAX_JOBS_PER_DAG
        || edges.len() > crate::domain::MAX_DEPENDENCIES_PER_DAG
    {
        return Err(ValidationError::InvalidJobCount);
    }
    let jobs: BTreeMap<_, _> = snapshot
        .jobs
        .iter()
        .map(|job| (job.id.as_str(), job))
        .collect();
    if jobs.len() != snapshot.jobs.len() {
        return Err(ValidationError::DuplicateJob("snapshot".into()));
    }
    for edge in edges {
        for id in [&edge.from, &edge.to] {
            if !jobs.contains_key(id.as_str()) {
                return Err(ValidationError::UnknownJob(id.clone()));
            }
        }
    }
    let parents = parent_map(edges);
    let mut rows = Vec::new();
    let mut remaining_blocker_ids = MAX_BLOCKER_IDS_PER_REPORT;
    for job in &snapshot.jobs {
        let direct = parents.get(job.id.as_str()).cloned().unwrap_or_default();
        let unmet_parents = direct
            .iter()
            .filter(|id| jobs[**id].state != JobState::Succeeded)
            .map(|id| (*id).to_owned())
            .collect();
        let mut seen = BTreeSet::new();
        // A terminal job is no longer awaiting dependency release.
        let mut pending: Vec<_> = if matches!(
            job.state,
            JobState::Succeeded | JobState::DeadLettered | JobState::Cancelled
        ) {
            Vec::new()
        } else {
            direct.into_iter().collect()
        };
        let mut terminal = BTreeSet::new();
        while let Some(parent) = pending.pop() {
            if !seen.insert(parent) {
                continue;
            }
            let state = &jobs[parent].state;
            if matches!(state, JobState::DeadLettered | JobState::Cancelled) {
                terminal.insert(parent.to_owned());
            }
            // A succeeded parent has already satisfied this edge; older ancestry does not block it.
            if *state != JobState::Succeeded {
                pending.extend(parents.get(parent).into_iter().flatten().copied());
            }
        }
        let terminal_blocker_count = terminal.len();
        let shown = terminal_blocker_count
            .min(MAX_BLOCKER_IDS_PER_JOB)
            .min(remaining_blocker_ids);
        remaining_blocker_ids -= shown;
        rows.push(JobExplanation {
            id: job.id.clone(),
            unmet_parents,
            terminal_blockers: terminal.into_iter().take(shown).collect(),
            terminal_blocker_count,
            terminal_blockers_truncated: shown < terminal_blocker_count,
            retry_wait_remaining_ms: (job.state == JobState::RetryWait)
                .then(|| job.available_at_ms.saturating_sub(observed_at_ms).max(0)),
            lease_remaining_ms: (job.state == JobState::Leased)
                .then(|| {
                    job.lease_expires_at_ms
                        .map(|at| at.saturating_sub(observed_at_ms).max(0))
                })
                .flatten(),
            lease_expired_at_snapshot: job.state == JobState::Leased
                && job
                    .lease_expires_at_ms
                    .is_some_and(|at| at <= observed_at_ms),
        });
    }
    Ok(rows)
}

/// One captured PostgreSQL snapshot plus a bounded newest-first event window.
#[derive(Debug, Serialize)]
pub struct DagInspection {
    pub snapshot: DagSnapshot,
    pub observed_at_ms: i64,
    pub dependencies: Vec<Dependency>,
    pub explanations: Vec<JobExplanation>,
    pub events: Vec<ObservedEvent>,
    pub events_truncated: bool,
    pub notice: &'static str,
}

/// Metadata only: lease tokens, payloads, results and arbitrary JSON details are excluded.
#[derive(Debug, Serialize)]
pub struct ObservedEvent {
    pub sequence: String,
    pub job_id: String,
    pub event_type: String,
    pub attempt: i16,
    pub occurred_at_ms: i64,
}
