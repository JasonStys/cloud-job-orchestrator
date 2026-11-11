//! File: Public library boundary for DAG validation, scheduling, persistence, and HTTP delivery.
//! Modules: `domain`, `scheduler`, `store`, `http`, and `worker`.
//! Variables: no mutable global state; callers own scheduler or database state explicitly.

pub mod database;
pub mod domain;
pub mod http;
pub mod inspection;
pub mod native_database;
pub mod native_http;
pub mod native_profile;
pub mod native_runtime;
pub mod scheduler;
pub mod store;
pub mod worker;

pub use domain::{DagSnapshot, DagSpec, Dependency, JobKind, JobSpec, JobState};
pub use scheduler::{ClaimedJob, Scheduler, SchedulerConfig, SchedulerError};
pub use store::{PostgresStore, StoreError};
