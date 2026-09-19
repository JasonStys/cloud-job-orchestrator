//! File: Implements the deterministic in-memory reference scheduler and lease state machine.
//! Functions: submit, claim, heartbeat, complete, fail, reap, cancel, and snapshot.
//! Variables: `dags` owns graph state; `ready` is an indexed min-order heap; `sequence` makes lease tokens unique.

use crate::domain::{DagSnapshot, DagSpec, JobSnapshot, JobSpec, JobState, ValidationError};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerConfig {
    pub max_dags: usize,
    pub max_ready_jobs: usize,
    pub max_lease_ms: i64,
    pub base_retry_ms: i64,
    pub max_retry_ms: i64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_dags: 100,
            max_ready_jobs: 10_000,
            max_lease_ms: 60_000,
            base_retry_ms: 1_000,
            max_retry_ms: 60_000,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ClaimedJob {
    pub dag_id: String,
    pub job_id: String,
    pub kind: crate::domain::JobKind,
    pub payload: String,
    pub attempt: u16,
    pub lease_token: String,
    pub lease_expires_at_ms: i64,
}

#[derive(Clone, Debug)]
struct Lease {
    owner: String,
    token: String,
    expires_at_ms: i64,
}

#[derive(Clone, Debug)]
struct JobRecord {
    spec: JobSpec,
    state: JobState,
    attempts: u16,
    available_at_ms: i64,
    remaining_dependencies: usize,
    lease: Option<Lease>,
    result: Option<String>,
}

#[derive(Clone, Debug)]
struct DagRecord {
    jobs: HashMap<String, JobRecord>,
    dependents: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ReadyKey {
    available_at_ms: i64,
    priority_rank: i16,
    dag_id: String,
    job_id: String,
}

#[derive(Debug)]
pub struct Scheduler {
    config: SchedulerConfig,
    dags: HashMap<String, DagRecord>,
    ready: BinaryHeap<Reverse<ReadyKey>>,
    sequence: u64,
}

impl Scheduler {
    /// Creates an empty scheduler with explicit admission and timing limits.
    pub fn new(config: SchedulerConfig) -> Self {
        Self {
            config,
            dags: HashMap::new(),
            ready: BinaryHeap::new(),
            sequence: 0,
        }
    }

    /// Validates and admits a DAG, initializing zero-dependency jobs as ready.
    pub fn submit(&mut self, spec: DagSpec, now_ms: i64) -> Result<(), SchedulerError> {
        spec.validate()?;
        if self.dags.contains_key(&spec.id) {
            return Err(SchedulerError::DagAlreadyExists);
        }
        if self.dags.len() >= self.config.max_dags {
            return Err(SchedulerError::Overloaded);
        }

        let mut dependency_counts: HashMap<String, usize> =
            spec.jobs.iter().map(|job| (job.id.clone(), 0)).collect();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &spec.dependencies {
            if let Some(count) = dependency_counts.get_mut(&edge.to) {
                *count += 1;
            }
            dependents
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
        }
        for values in dependents.values_mut() {
            values.sort_unstable();
        }

        let ready_count = dependency_counts
            .values()
            .filter(|count| **count == 0)
            .count();
        if self.ready.len().saturating_add(ready_count) > self.config.max_ready_jobs {
            return Err(SchedulerError::Overloaded);
        }

        let dag_id = spec.id.clone();
        let mut jobs = HashMap::with_capacity(spec.jobs.len());
        for job in spec.jobs {
            let remaining_dependencies =
                dependency_counts.get(&job.id).copied().unwrap_or_default();
            let state = if remaining_dependencies == 0 {
                JobState::Ready
            } else {
                JobState::Pending
            };
            if state == JobState::Ready {
                self.push_ready(&dag_id, &job, now_ms);
            }
            jobs.insert(
                job.id.clone(),
                JobRecord {
                    spec: job,
                    state,
                    attempts: 0,
                    available_at_ms: now_ms,
                    remaining_dependencies,
                    lease: None,
                    result: None,
                },
            );
        }
        self.dags.insert(dag_id, DagRecord { jobs, dependents });
        Ok(())
    }

    /// Claims the highest-priority ready job whose availability time has passed.
    pub fn claim(
        &mut self,
        owner: &str,
        now_ms: i64,
        lease_ms: i64,
    ) -> Result<Option<ClaimedJob>, SchedulerError> {
        if owner.is_empty()
            || owner.len() > 64
            || lease_ms <= 0
            || lease_ms > self.config.max_lease_ms
        {
            return Err(SchedulerError::InvalidLease);
        }
        self.reap(now_ms);
        loop {
            let Some(Reverse(key)) = self.ready.peek() else {
                return Ok(None);
            };
            if key.available_at_ms > now_ms {
                return Ok(None);
            }
            let Some(Reverse(key)) = self.ready.pop() else {
                return Ok(None);
            };
            let Some(record) = self.dags.get_mut(&key.dag_id) else {
                continue;
            };
            let Some(job) = record.jobs.get_mut(&key.job_id) else {
                continue;
            };
            if !matches!(job.state, JobState::Ready | JobState::RetryWait)
                || job.available_at_ms != key.available_at_ms
            {
                continue;
            }
            self.sequence = self.sequence.saturating_add(1);
            let token = format!("lease-{}-{}-{}", key.dag_id, key.job_id, self.sequence);
            let expires_at_ms = now_ms.saturating_add(lease_ms);
            job.attempts = job.attempts.saturating_add(1);
            job.state = JobState::Leased;
            job.lease = Some(Lease {
                owner: owner.to_owned(),
                token: token.clone(),
                expires_at_ms,
            });
            return Ok(Some(ClaimedJob {
                dag_id: key.dag_id,
                job_id: key.job_id,
                kind: job.spec.kind.clone(),
                payload: job.spec.payload.clone(),
                attempt: job.attempts,
                lease_token: token,
                lease_expires_at_ms: expires_at_ms,
            }));
        }
    }

    /// Extends a live lease only when both its owner and unguessable token match.
    pub fn heartbeat(
        &mut self,
        dag_id: &str,
        job_id: &str,
        owner: &str,
        token: &str,
        now_ms: i64,
        lease_ms: i64,
    ) -> Result<i64, SchedulerError> {
        if lease_ms <= 0 || lease_ms > self.config.max_lease_ms {
            return Err(SchedulerError::InvalidLease);
        }
        let job = self.job_mut(dag_id, job_id)?;
        let Some(lease) = job.lease.as_mut() else {
            return Err(SchedulerError::LeaseConflict);
        };
        if job.state != JobState::Leased
            || lease.owner != owner
            || lease.token != token
            || lease.expires_at_ms <= now_ms
        {
            return Err(SchedulerError::LeaseConflict);
        }
        lease.expires_at_ms = now_ms.saturating_add(lease_ms);
        Ok(lease.expires_at_ms)
    }

    /// Commits a success using compare-and-set lease semantics and releases dependents.
    pub fn complete(
        &mut self,
        dag_id: &str,
        job_id: &str,
        owner: &str,
        token: &str,
        result: String,
        now_ms: i64,
    ) -> Result<(), SchedulerError> {
        if result.len() > crate::domain::MAX_PAYLOAD_BYTES {
            return Err(SchedulerError::ResultTooLarge);
        }
        self.assert_lease(dag_id, job_id, owner, token, now_ms)?;
        let dependents = {
            let dag = self
                .dags
                .get_mut(dag_id)
                .ok_or(SchedulerError::DagNotFound)?;
            let job = dag
                .jobs
                .get_mut(job_id)
                .ok_or(SchedulerError::JobNotFound)?;
            job.state = JobState::Succeeded;
            job.lease = None;
            job.result = Some(result);
            dag.dependents.get(job_id).cloned().unwrap_or_default()
        };
        for dependent in dependents {
            let ready_spec = {
                let dag = self
                    .dags
                    .get_mut(dag_id)
                    .ok_or(SchedulerError::DagNotFound)?;
                let job = dag
                    .jobs
                    .get_mut(&dependent)
                    .ok_or(SchedulerError::JobNotFound)?;
                job.remaining_dependencies = job.remaining_dependencies.saturating_sub(1);
                if job.remaining_dependencies == 0 && job.state == JobState::Pending {
                    job.state = JobState::Ready;
                    job.available_at_ms = now_ms;
                    Some(job.spec.clone())
                } else {
                    None
                }
            };
            if let Some(spec) = ready_spec {
                self.push_ready(dag_id, &spec, now_ms);
            }
        }
        Ok(())
    }

    /// Records a failure and either applies bounded backoff or dead-letters the job.
    pub fn fail(
        &mut self,
        dag_id: &str,
        job_id: &str,
        owner: &str,
        token: &str,
        now_ms: i64,
    ) -> Result<JobState, SchedulerError> {
        self.assert_lease(dag_id, job_id, owner, token, now_ms)?;
        self.retry_or_dead_letter(dag_id, job_id, now_ms)
    }

    /// Reclaims expired leases. Late workers cannot commit after this transition.
    pub fn reap(&mut self, now_ms: i64) -> usize {
        let expired: Vec<(String, String)> = self
            .dags
            .iter()
            .flat_map(|(dag_id, dag)| {
                dag.jobs.iter().filter_map(move |(job_id, job)| {
                    job.lease
                        .as_ref()
                        .filter(|lease| {
                            job.state == JobState::Leased && lease.expires_at_ms <= now_ms
                        })
                        .map(|_| (dag_id.clone(), job_id.clone()))
                })
            })
            .collect();
        for (dag_id, job_id) in &expired {
            let _ = self.retry_or_dead_letter(dag_id, job_id, now_ms);
        }
        expired.len()
    }

    /// Cancels every non-terminal job in a DAG and invalidates active leases.
    pub fn cancel(&mut self, dag_id: &str) -> Result<usize, SchedulerError> {
        let dag = self
            .dags
            .get_mut(dag_id)
            .ok_or(SchedulerError::DagNotFound)?;
        let mut cancelled = 0;
        for job in dag.jobs.values_mut() {
            if !matches!(
                job.state,
                JobState::Succeeded | JobState::DeadLettered | JobState::Cancelled
            ) {
                job.state = JobState::Cancelled;
                job.lease = None;
                cancelled += 1;
            }
        }
        Ok(cancelled)
    }

    /// Returns a stable, identifier-sorted read model.
    pub fn snapshot(&self, dag_id: &str) -> Result<DagSnapshot, SchedulerError> {
        let dag = self.dags.get(dag_id).ok_or(SchedulerError::DagNotFound)?;
        let mut jobs: Vec<JobSnapshot> = dag
            .jobs
            .values()
            .map(|job| JobSnapshot {
                id: job.spec.id.clone(),
                state: job.state.clone(),
                attempts: job.attempts,
                max_attempts: job.spec.max_attempts,
                available_at_ms: job.available_at_ms,
                lease_owner: job.lease.as_ref().map(|lease| lease.owner.clone()),
                lease_expires_at_ms: job.lease.as_ref().map(|lease| lease.expires_at_ms),
                result: job.result.clone(),
            })
            .collect();
        jobs.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(DagSnapshot {
            id: dag_id.to_owned(),
            jobs,
        })
    }

    fn retry_or_dead_letter(
        &mut self,
        dag_id: &str,
        job_id: &str,
        now_ms: i64,
    ) -> Result<JobState, SchedulerError> {
        let base_retry_ms = self.config.base_retry_ms;
        let max_retry_ms = self.config.max_retry_ms;
        let (state, available_at_ms, spec) = {
            let job = self.job_mut(dag_id, job_id)?;
            job.lease = None;
            if job.attempts >= job.spec.max_attempts {
                job.state = JobState::DeadLettered;
                (JobState::DeadLettered, now_ms, None)
            } else {
                let exponent = u32::from(job.attempts.saturating_sub(1).min(16));
                let base = base_retry_ms.saturating_mul(1_i64 << exponent);
                let bounded = base.min(max_retry_ms);
                let jitter = deterministic_jitter(job_id, bounded / 5);
                job.available_at_ms = now_ms.saturating_add(bounded).saturating_add(jitter);
                job.state = JobState::RetryWait;
                (
                    JobState::RetryWait,
                    job.available_at_ms,
                    Some(job.spec.clone()),
                )
            }
        };
        if let Some(spec) = spec {
            self.push_ready(dag_id, &spec, available_at_ms);
        }
        Ok(state)
    }

    fn assert_lease(
        &mut self,
        dag_id: &str,
        job_id: &str,
        owner: &str,
        token: &str,
        now_ms: i64,
    ) -> Result<(), SchedulerError> {
        let job = self.job_mut(dag_id, job_id)?;
        let Some(lease) = &job.lease else {
            return Err(SchedulerError::LeaseConflict);
        };
        if job.state == JobState::Leased
            && lease.owner == owner
            && lease.token == token
            && lease.expires_at_ms > now_ms
        {
            Ok(())
        } else {
            Err(SchedulerError::LeaseConflict)
        }
    }

    fn job_mut(&mut self, dag_id: &str, job_id: &str) -> Result<&mut JobRecord, SchedulerError> {
        self.dags
            .get_mut(dag_id)
            .ok_or(SchedulerError::DagNotFound)?
            .jobs
            .get_mut(job_id)
            .ok_or(SchedulerError::JobNotFound)
    }

    fn push_ready(&mut self, dag_id: &str, job: &JobSpec, available_at_ms: i64) {
        self.ready.push(Reverse(ReadyKey {
            available_at_ms,
            priority_rank: 100_i16.saturating_sub(job.priority),
            dag_id: dag_id.to_owned(),
            job_id: job.id.clone(),
        }));
    }
}

fn deterministic_jitter(job_id: &str, ceiling: i64) -> i64 {
    if ceiling <= 0 {
        return 0;
    }
    let hash = job_id.bytes().fold(0_u64, |value, byte| {
        value.wrapping_mul(31).wrapping_add(u64::from(byte))
    });
    i64::try_from(hash % u64::try_from(ceiling).unwrap_or(1)).unwrap_or_default()
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SchedulerError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("DAG already exists")]
    DagAlreadyExists,
    #[error("DAG not found")]
    DagNotFound,
    #[error("job not found")]
    JobNotFound,
    #[error("admission limit exceeded")]
    Overloaded,
    #[error("lease duration or owner is invalid")]
    InvalidLease,
    #[error("lease is missing, expired, or owned by another worker")]
    LeaseConflict,
    #[error("result exceeds the payload limit")]
    ResultTooLarge,
}

#[cfg(test)]
mod tests {
    use super::{Scheduler, SchedulerConfig, SchedulerError};
    use crate::domain::{DagSpec, Dependency, JobKind, JobSpec, JobState};

    fn job(id: &str, priority: i16, max_attempts: u16) -> JobSpec {
        JobSpec {
            id: id.to_owned(),
            kind: JobKind::Uppercase,
            payload: id.to_owned(),
            max_attempts,
            priority,
        }
    }

    fn chain() -> DagSpec {
        DagSpec {
            id: "chain".to_owned(),
            jobs: vec![job("extract", 1, 3), job("publish", 0, 3)],
            dependencies: vec![Dependency {
                from: "extract".to_owned(),
                to: "publish".to_owned(),
            }],
        }
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn dependent_is_released_only_after_success() {
        let mut scheduler = Scheduler::new(SchedulerConfig::default());
        scheduler.submit(chain(), 0).unwrap();
        let first = scheduler.claim("worker-a", 1, 100).unwrap().unwrap();
        assert_eq!(first.job_id, "extract");
        assert_eq!(scheduler.claim("worker-b", 2, 100), Ok(None));
        scheduler
            .complete(
                &first.dag_id,
                &first.job_id,
                "worker-a",
                &first.lease_token,
                "ok".to_owned(),
                3,
            )
            .unwrap();
        let second = scheduler.claim("worker-b", 4, 100).unwrap().unwrap();
        assert_eq!(second.job_id, "publish");
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn expired_lease_retries_and_rejects_late_completion() {
        let mut scheduler = Scheduler::new(SchedulerConfig {
            base_retry_ms: 10,
            max_retry_ms: 10,
            ..SchedulerConfig::default()
        });
        scheduler.submit(chain(), 0).unwrap();
        let claim = scheduler.claim("dead-worker", 0, 5).unwrap().unwrap();
        assert_eq!(scheduler.reap(5), 1);
        assert_eq!(
            scheduler.complete(
                "chain",
                "extract",
                "dead-worker",
                &claim.lease_token,
                "late".to_owned(),
                5
            ),
            Err(SchedulerError::LeaseConflict)
        );
        let snapshot = scheduler.snapshot("chain").unwrap();
        assert_eq!(snapshot.jobs[0].state, JobState::RetryWait);
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn dead_letters_after_attempt_budget() {
        let mut scheduler = Scheduler::new(SchedulerConfig {
            base_retry_ms: 1,
            max_retry_ms: 1,
            ..SchedulerConfig::default()
        });
        let dag = DagSpec {
            id: "dlq".to_owned(),
            jobs: vec![job("fragile", 0, 1)],
            dependencies: vec![],
        };
        scheduler.submit(dag, 0).unwrap();
        let claim = scheduler.claim("worker", 0, 5).unwrap().unwrap();
        assert_eq!(
            scheduler.fail("dlq", "fragile", "worker", &claim.lease_token, 1),
            Ok(JobState::DeadLettered)
        );
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn cancellation_invalidates_a_live_lease() {
        let mut scheduler = Scheduler::new(SchedulerConfig::default());
        scheduler.submit(chain(), 0).unwrap();
        let claim = scheduler.claim("worker", 0, 100).unwrap().unwrap();
        assert_eq!(scheduler.cancel("chain"), Ok(2));
        assert_eq!(
            scheduler.heartbeat("chain", "extract", "worker", &claim.lease_token, 1, 100),
            Err(SchedulerError::LeaseConflict)
        );
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn higher_priority_job_is_claimed_first() {
        let mut scheduler = Scheduler::new(SchedulerConfig::default());
        scheduler
            .submit(
                DagSpec {
                    id: "priorities".to_owned(),
                    jobs: vec![job("low", -5, 1), job("high", 10, 1)],
                    dependencies: vec![],
                },
                0,
            )
            .unwrap();
        assert_eq!(
            scheduler.claim("worker", 0, 100).unwrap().unwrap().job_id,
            "high"
        );
    }
}
