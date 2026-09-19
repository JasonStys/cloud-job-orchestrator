//! File: Defines serializable job, graph, lease, and snapshot value objects.
//! Functions: `JobKind::validate_payload` constrains synthetic work; `DagSpec::validate` validates graph input.
//! Variables: identifiers, graph cardinality, and payloads have explicit admission limits.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use thiserror::Error;

pub const MAX_DAG_ID_BYTES: usize = 64;
pub const MAX_JOB_ID_BYTES: usize = 64;
pub const MAX_JOBS_PER_DAG: usize = 1_000;
pub const MAX_DEPENDENCIES_PER_DAG: usize = 10_000;
pub const MAX_PAYLOAD_BYTES: usize = 4_096;

/// Fixed, harmless worker operations. Arbitrary commands are intentionally not representable.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Delay,
    Checksum,
    Uppercase,
}

impl JobKind {
    /// Checks operation-specific payload limits before work enters the queue.
    pub fn validate_payload(&self, payload: &str) -> Result<(), ValidationError> {
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(ValidationError::PayloadTooLarge);
        }
        if matches!(self, Self::Delay) {
            let delay = payload
                .parse::<u64>()
                .map_err(|_| ValidationError::InvalidDelay)?;
            if delay > 5_000 {
                return Err(ValidationError::InvalidDelay);
            }
        }
        Ok(())
    }
}

/// Declarative unit of bounded work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JobSpec {
    pub id: String,
    pub kind: JobKind,
    pub payload: String,
    #[serde(default = "default_attempts")]
    pub max_attempts: u16,
    #[serde(default)]
    pub priority: i16,
}

const fn default_attempts() -> u16 {
    3
}

/// Directed edge where `from` must succeed before `to` can run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Dependency {
    pub from: String,
    pub to: String,
}

/// Complete user-submitted directed acyclic graph.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DagSpec {
    pub id: String,
    pub jobs: Vec<JobSpec>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

impl DagSpec {
    /// Validates identifiers, payloads, edge endpoints, and acyclicity in O(V + E).
    pub fn validate(&self) -> Result<Vec<String>, ValidationError> {
        validate_identifier(&self.id, MAX_DAG_ID_BYTES)?;
        if self.jobs.is_empty() || self.jobs.len() > MAX_JOBS_PER_DAG {
            return Err(ValidationError::InvalidJobCount);
        }
        if self.dependencies.len() > MAX_DEPENDENCIES_PER_DAG {
            return Err(ValidationError::InvalidDependencyCount);
        }

        let mut indegree = HashMap::new();
        let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();
        for job in &self.jobs {
            validate_identifier(&job.id, MAX_JOB_ID_BYTES)?;
            job.kind.validate_payload(&job.payload)?;
            if job.max_attempts == 0
                || job.max_attempts > 20
                || !(-100..=100).contains(&job.priority)
            {
                return Err(ValidationError::InvalidPolicy);
            }
            if indegree.insert(job.id.as_str(), 0_usize).is_some() {
                return Err(ValidationError::DuplicateJob(job.id.clone()));
            }
        }

        for edge in &self.dependencies {
            if edge.from == edge.to {
                return Err(ValidationError::Cycle);
            }
            if !indegree.contains_key(edge.from.as_str()) {
                return Err(ValidationError::UnknownJob(edge.from.clone()));
            }
            let Some(target_degree) = indegree.get_mut(edge.to.as_str()) else {
                return Err(ValidationError::UnknownJob(edge.to.clone()));
            };
            *target_degree += 1;
            outgoing
                .entry(edge.from.as_str())
                .or_default()
                .push(edge.to.as_str());
        }

        let mut zero: Vec<&str> = indegree
            .iter()
            .filter_map(|(id, count)| (*count == 0).then_some(*id))
            .collect();
        zero.sort_unstable();
        let mut queue: VecDeque<&str> = zero.into();
        let mut order = Vec::new();
        while let Some(id) = queue.pop_front() {
            order.push(id.to_owned());
            if let Some(children) = outgoing.get(id) {
                let mut newly_ready = Vec::new();
                for child in children {
                    if let Some(count) = indegree.get_mut(child) {
                        *count -= 1;
                        if *count == 0 {
                            newly_ready.push(*child);
                        }
                    }
                }
                newly_ready.sort_unstable();
                queue.extend(newly_ready);
            }
        }
        if order.len() != self.jobs.len() {
            return Err(ValidationError::Cycle);
        }
        Ok(order)
    }
}

fn validate_identifier(value: &str, max_bytes: usize) -> Result<(), ValidationError> {
    let valid = !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(ValidationError::InvalidIdentifier(value.to_owned()))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Ready,
    RetryWait,
    Leased,
    Succeeded,
    DeadLettered,
    Cancelled,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct JobSnapshot {
    pub id: String,
    pub state: JobState,
    pub attempts: u16,
    pub max_attempts: u16,
    pub available_at_ms: i64,
    pub lease_owner: Option<String>,
    pub lease_expires_at_ms: Option<i64>,
    pub result: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DagSnapshot {
    pub id: String,
    pub jobs: Vec<JobSnapshot>,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ValidationError {
    #[error("identifier must contain 1..=64 ASCII letters, numbers, dashes, or underscores: {0}")]
    InvalidIdentifier(String),
    #[error("DAG must contain 1..=1000 jobs")]
    InvalidJobCount,
    #[error("DAG must contain at most 10000 dependencies")]
    InvalidDependencyCount,
    #[error("duplicate job identifier: {0}")]
    DuplicateJob(String),
    #[error("dependency references unknown job: {0}")]
    UnknownJob(String),
    #[error("dependency graph contains a cycle")]
    Cycle,
    #[error("payload exceeds 4096 bytes")]
    PayloadTooLarge,
    #[error("delay payload must be an integer from 0 through 5000 milliseconds")]
    InvalidDelay,
    #[error("attempts must be 1..=20 and priority must be -100..=100")]
    InvalidPolicy,
}

#[cfg(test)]
mod tests {
    use super::{DagSpec, Dependency, JobKind, JobSpec, MAX_DEPENDENCIES_PER_DAG, ValidationError};

    fn job(id: &str) -> JobSpec {
        JobSpec {
            id: id.to_owned(),
            kind: JobKind::Uppercase,
            payload: "safe".to_owned(),
            max_attempts: 3,
            priority: 0,
        }
    }

    #[test]
    fn topological_order_is_stable() {
        let dag = DagSpec {
            id: "release".to_owned(),
            jobs: vec![job("c"), job("b"), job("a")],
            dependencies: vec![
                Dependency {
                    from: "a".to_owned(),
                    to: "c".to_owned(),
                },
                Dependency {
                    from: "b".to_owned(),
                    to: "c".to_owned(),
                },
            ],
        };
        assert_eq!(
            dag.validate(),
            Ok(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()])
        );
    }

    #[test]
    fn cycle_is_rejected() {
        let dag = DagSpec {
            id: "cycle".to_owned(),
            jobs: vec![job("a"), job("b")],
            dependencies: vec![
                Dependency {
                    from: "a".to_owned(),
                    to: "b".to_owned(),
                },
                Dependency {
                    from: "b".to_owned(),
                    to: "a".to_owned(),
                },
            ],
        };
        assert_eq!(dag.validate(), Err(ValidationError::Cycle));
    }

    #[test]
    fn excessive_dependency_count_is_rejected_before_graph_allocation() {
        let dependency = Dependency {
            from: "a".to_owned(),
            to: "b".to_owned(),
        };
        let dag = DagSpec {
            id: "bounded".to_owned(),
            jobs: vec![job("a"), job("b")],
            dependencies: vec![dependency; MAX_DEPENDENCIES_PER_DAG + 1],
        };

        assert_eq!(dag.validate(), Err(ValidationError::InvalidDependencyCount));
    }
}
