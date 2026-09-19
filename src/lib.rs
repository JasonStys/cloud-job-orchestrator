//! File: Public library boundary for DAG validation, scheduling, persistence, and HTTP delivery.
//! Modules: `domain`, `scheduler`, `store`, `http`, and `worker`.
//! Variables: no mutable global state; callers own scheduler or database state explicitly.

pub mod domain;
pub mod http;
pub mod scheduler;
pub mod store;
pub mod worker;

pub use domain::{DagSnapshot, DagSpec, Dependency, JobKind, JobSpec, JobState};
pub use scheduler::{ClaimedJob, Scheduler, SchedulerConfig, SchedulerError};
pub use store::{PostgresStore, StoreError};
