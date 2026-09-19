//! File: Implements durable PostgreSQL transactions for DAG submission and worker leases.
//! Functions: connect, migrate, submit, claim, heartbeat, complete, fail, reap, cancel, snapshot, and list.
//! Variables: `pool` is a bounded, cloneable connection pool; all state changes are database transactions.

use crate::domain::{DagSnapshot, DagSpec, JobKind, JobSnapshot, JobState, ValidationError};
use crate::scheduler::ClaimedJob;
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgPoolOptions};
use std::time::Duration;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    /// Opens a bounded pool with fail-fast acquisition behavior.
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, StoreError> {
        if !(1..=100).contains(&max_connections) {
            return Err(StoreError::InvalidConfiguration);
        }
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    /// Applies embedded, checksummed migrations before serving traffic.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!().run(&self.pool).await?;
        Ok(())
    }

    /// Checks that a connection can execute a trivial query.
    pub async fn ready(&self) -> bool {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }

    /// Persists a fully validated DAG in one atomic transaction.
    pub async fn submit(&self, spec: &DagSpec) -> Result<(), StoreError> {
        spec.validate()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO dags (id) VALUES ($1)")
            .bind(&spec.id)
            .execute(&mut *tx)
            .await?;

        let mut dependencies = std::collections::HashMap::<&str, usize>::new();
        for job in &spec.jobs {
            dependencies.insert(&job.id, 0);
        }
        for edge in &spec.dependencies {
            if let Some(value) = dependencies.get_mut(edge.to.as_str()) {
                *value += 1;
            }
        }
        for job in &spec.jobs {
            let state = if dependencies
                .get(job.id.as_str())
                .copied()
                .unwrap_or_default()
                == 0
            {
                "ready"
            } else {
                "pending"
            };
            sqlx::query(
                "INSERT INTO jobs (dag_id, id, kind, payload, max_attempts, priority, state) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(&spec.id)
            .bind(&job.id)
            .bind(kind_name(&job.kind))
            .bind(&job.payload)
            .bind(i16::try_from(job.max_attempts).map_err(|_| StoreError::InvalidConfiguration)?)
            .bind(job.priority)
            .bind(state)
            .execute(&mut *tx)
            .await?;
        }
        for edge in &spec.dependencies {
            sqlx::query(
                "INSERT INTO job_dependencies (dag_id, parent_id, child_id) VALUES ($1, $2, $3)",
            )
            .bind(&spec.id)
            .bind(&edge.from)
            .bind(&edge.to)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Atomically reaps expired leases and claims one eligible job using `SKIP LOCKED`.
    pub async fn claim(
        &self,
        owner: &str,
        lease_ms: i64,
    ) -> Result<Option<ClaimedJob>, StoreError> {
        validate_owner_and_lease(owner, lease_ms)?;
        let mut tx = self.pool.begin().await?;
        self.reap_in_transaction(&mut tx).await?;
        let token = Uuid::new_v4();
        let row = sqlx::query(
            "WITH candidate AS ( \
                 SELECT j.dag_id, j.id FROM jobs j \
                 JOIN dags d ON d.id = j.dag_id AND d.state = 'running' \
                 WHERE j.state IN ('ready', 'retry_wait') \
                   AND j.available_at <= CURRENT_TIMESTAMP \
                   AND NOT EXISTS ( \
                     SELECT 1 FROM job_dependencies dep \
                     JOIN jobs parent ON parent.dag_id = dep.dag_id AND parent.id = dep.parent_id \
                     WHERE dep.dag_id = j.dag_id AND dep.child_id = j.id AND parent.state <> 'succeeded' \
                   ) \
                 ORDER BY j.priority DESC, j.available_at, j.dag_id, j.id \
                 FOR UPDATE OF j SKIP LOCKED LIMIT 1 \
             ) \
             UPDATE jobs j SET state = 'leased', attempts = attempts + 1, lease_owner = $1, \
                 lease_token = $2, lease_expires_at = CURRENT_TIMESTAMP + ($3::bigint * interval '1 millisecond'), \
                 updated_at = CURRENT_TIMESTAMP \
             FROM candidate c WHERE j.dag_id = c.dag_id AND j.id = c.id \
             RETURNING j.dag_id, j.id, j.kind, j.payload, j.attempts, j.lease_expires_at",
        )
        .bind(owner)
        .bind(token)
        .bind(lease_ms)
        .fetch_optional(&mut *tx)
        .await?;

        let claim = if let Some(row) = row {
            let dag_id: String = row.try_get("dag_id")?;
            let job_id: String = row.try_get("id")?;
            let attempt: i16 = row.try_get("attempts")?;
            let expires: OffsetDateTime = row.try_get("lease_expires_at")?;
            sqlx::query(
                "INSERT INTO job_events (dag_id, job_id, event_type, attempt, detail) \
                 VALUES ($1, $2, 'claimed', $3, jsonb_build_object('owner', $4))",
            )
            .bind(&dag_id)
            .bind(&job_id)
            .bind(attempt)
            .bind(owner)
            .execute(&mut *tx)
            .await?;
            Some(ClaimedJob {
                dag_id,
                job_id,
                kind: parse_kind(row.try_get::<String, _>("kind")?.as_str())?,
                payload: row.try_get("payload")?,
                attempt: u16::try_from(attempt).map_err(|_| StoreError::CorruptData)?,
                lease_token: token.to_string(),
                lease_expires_at_ms: unix_millis(expires),
            })
        } else {
            None
        };
        tx.commit().await?;
        Ok(claim)
    }

    /// Extends a lease only when the current owner, token, state, and deadline match.
    pub async fn heartbeat(
        &self,
        dag_id: &str,
        job_id: &str,
        owner: &str,
        token: Uuid,
        lease_ms: i64,
    ) -> Result<(), StoreError> {
        validate_owner_and_lease(owner, lease_ms)?;
        let changed = sqlx::query(
            "UPDATE jobs SET lease_expires_at = CURRENT_TIMESTAMP + ($5::bigint * interval '1 millisecond'), \
             updated_at = CURRENT_TIMESTAMP WHERE dag_id = $1 AND id = $2 AND state = 'leased' \
             AND lease_owner = $3 AND lease_token = $4 AND lease_expires_at > CURRENT_TIMESTAMP",
        )
        .bind(dag_id)
        .bind(job_id)
        .bind(owner)
        .bind(token)
        .bind(lease_ms)
        .execute(&self.pool)
        .await?
        .rows_affected();
        ensure_compare_and_set(changed)
    }

    /// Commits a result and releases dependency-satisfied children atomically.
    pub async fn complete(
        &self,
        dag_id: &str,
        job_id: &str,
        owner: &str,
        token: Uuid,
        result: &str,
    ) -> Result<(), StoreError> {
        if result.len() > crate::domain::MAX_PAYLOAD_BYTES {
            return Err(StoreError::InvalidConfiguration);
        }
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(
            "UPDATE jobs SET state = 'succeeded', result = $5, lease_owner = NULL, lease_token = NULL, \
             lease_expires_at = NULL, updated_at = CURRENT_TIMESTAMP \
             WHERE dag_id = $1 AND id = $2 AND state = 'leased' AND lease_owner = $3 \
             AND lease_token = $4 AND lease_expires_at > CURRENT_TIMESTAMP",
        )
        .bind(dag_id)
        .bind(job_id)
        .bind(owner)
        .bind(token)
        .bind(result)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        ensure_compare_and_set(changed)?;
        sqlx::query(
            "UPDATE jobs child SET state = 'ready', available_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP \
             WHERE child.dag_id = $1 AND child.state = 'pending' \
             AND EXISTS (SELECT 1 FROM job_dependencies edge WHERE edge.dag_id = child.dag_id \
                         AND edge.parent_id = $2 AND edge.child_id = child.id) \
             AND NOT EXISTS (SELECT 1 FROM job_dependencies edge JOIN jobs parent \
                 ON parent.dag_id = edge.dag_id AND parent.id = edge.parent_id \
                 WHERE edge.dag_id = child.dag_id AND edge.child_id = child.id AND parent.state <> 'succeeded')",
        )
        .bind(dag_id)
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
        insert_event(&mut tx, dag_id, job_id, "succeeded", owner).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Applies exponential backoff or moves an exhausted job to the dead-letter state.
    pub async fn fail(
        &self,
        dag_id: &str,
        job_id: &str,
        owner: &str,
        token: Uuid,
    ) -> Result<JobState, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "UPDATE jobs SET \
               state = CASE WHEN attempts >= max_attempts THEN 'dead_lettered' ELSE 'retry_wait' END, \
               available_at = CASE WHEN attempts >= max_attempts THEN available_at ELSE CURRENT_TIMESTAMP + \
                 (LEAST(60000, 1000 * power(2, LEAST(16, GREATEST(0, attempts - 1))))::bigint * interval '1 millisecond') END, \
               lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = CURRENT_TIMESTAMP \
             WHERE dag_id = $1 AND id = $2 AND state = 'leased' AND lease_owner = $3 \
               AND lease_token = $4 AND lease_expires_at > CURRENT_TIMESTAMP \
             RETURNING state",
        )
        .bind(dag_id)
        .bind(job_id)
        .bind(owner)
        .bind(token)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::LeaseConflict)?;
        let state = parse_state(row.try_get::<String, _>("state")?.as_str())?;
        insert_event(&mut tx, dag_id, job_id, state_name(&state), owner).await?;
        tx.commit().await?;
        Ok(state)
    }

    /// Reclaims all currently expired leases and returns the transition count.
    pub async fn reap(&self) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        let changed = self.reap_in_transaction(&mut tx).await?;
        tx.commit().await?;
        Ok(changed)
    }

    /// Cancels a DAG and invalidates every unfinished lease in one transaction.
    pub async fn cancel(&self, dag_id: &str) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await?;
        let dag_changed = sqlx::query(
            "UPDATE dags SET state = 'cancelled', updated_at = CURRENT_TIMESTAMP WHERE id = $1",
        )
        .bind(dag_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if dag_changed == 0 {
            return Err(StoreError::NotFound);
        }
        let changed = sqlx::query(
            "UPDATE jobs SET state = 'cancelled', lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, \
             updated_at = CURRENT_TIMESTAMP WHERE dag_id = $1 \
             AND state NOT IN ('succeeded', 'dead_lettered', 'cancelled')",
        )
        .bind(dag_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(changed)
    }

    /// Reads one DAG and its jobs in stable order.
    pub async fn snapshot(&self, dag_id: &str) -> Result<DagSnapshot, StoreError> {
        let exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM dags WHERE id = $1)")
                .bind(dag_id)
                .fetch_one(&self.pool)
                .await?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query(
            "SELECT id, state, attempts, max_attempts, available_at, lease_owner, lease_expires_at, result \
             FROM jobs WHERE dag_id = $1 ORDER BY id",
        )
        .bind(dag_id)
        .fetch_all(&self.pool)
        .await?;
        let mut jobs = Vec::with_capacity(rows.len());
        for row in rows {
            let attempts: i16 = row.try_get("attempts")?;
            let max_attempts: i16 = row.try_get("max_attempts")?;
            let available: OffsetDateTime = row.try_get("available_at")?;
            let lease_expires: Option<OffsetDateTime> = row.try_get("lease_expires_at")?;
            jobs.push(JobSnapshot {
                id: row.try_get("id")?,
                state: parse_state(row.try_get::<String, _>("state")?.as_str())?,
                attempts: u16::try_from(attempts).map_err(|_| StoreError::CorruptData)?,
                max_attempts: u16::try_from(max_attempts).map_err(|_| StoreError::CorruptData)?,
                available_at_ms: unix_millis(available),
                lease_owner: row.try_get("lease_owner")?,
                lease_expires_at_ms: lease_expires.map(unix_millis),
                result: row.try_get("result")?,
            });
        }
        Ok(DagSnapshot {
            id: dag_id.to_owned(),
            jobs,
        })
    }

    /// Lists DAG identifiers with keyset pagination and a hard page-size cap.
    pub async fn list_dags(
        &self,
        after: Option<&str>,
        limit: u16,
    ) -> Result<Vec<String>, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::InvalidConfiguration);
        }
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT id FROM dags WHERE id > COALESCE($1, '') ORDER BY id LIMIT $2",
        )
        .bind(after)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn reap_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<u64, StoreError> {
        let changed = sqlx::query(
            "UPDATE jobs SET \
               state = CASE WHEN attempts >= max_attempts THEN 'dead_lettered' ELSE 'retry_wait' END, \
               available_at = CASE WHEN attempts >= max_attempts THEN available_at ELSE CURRENT_TIMESTAMP + \
                 (LEAST(60000, 1000 * power(2, LEAST(16, GREATEST(0, attempts - 1))))::bigint * interval '1 millisecond') END, \
               lease_owner = NULL, lease_token = NULL, lease_expires_at = NULL, updated_at = CURRENT_TIMESTAMP \
             WHERE state = 'leased' AND lease_expires_at <= CURRENT_TIMESTAMP",
        )
        .execute(&mut **tx)
        .await?
        .rows_affected();
        Ok(changed)
    }
}

async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    dag_id: &str,
    job_id: &str,
    event_type: &str,
    owner: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO job_events (dag_id, job_id, event_type, attempt, detail) \
         SELECT dag_id, id, $3, attempts, jsonb_build_object('owner', $4) FROM jobs \
         WHERE dag_id = $1 AND id = $2",
    )
    .bind(dag_id)
    .bind(job_id)
    .bind(event_type)
    .bind(owner)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_owner_and_lease(owner: &str, lease_ms: i64) -> Result<(), StoreError> {
    if owner.is_empty() || owner.len() > 64 || !(1..=60_000).contains(&lease_ms) {
        Err(StoreError::InvalidConfiguration)
    } else {
        Ok(())
    }
}

fn ensure_compare_and_set(changed: u64) -> Result<(), StoreError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(StoreError::LeaseConflict)
    }
}

const fn kind_name(kind: &JobKind) -> &'static str {
    match kind {
        JobKind::Delay => "delay",
        JobKind::Checksum => "checksum",
        JobKind::Uppercase => "uppercase",
    }
}

fn parse_kind(value: &str) -> Result<JobKind, StoreError> {
    match value {
        "delay" => Ok(JobKind::Delay),
        "checksum" => Ok(JobKind::Checksum),
        "uppercase" => Ok(JobKind::Uppercase),
        _ => Err(StoreError::CorruptData),
    }
}

const fn state_name(state: &JobState) -> &'static str {
    match state {
        JobState::Pending => "pending",
        JobState::Ready => "ready",
        JobState::RetryWait => "retry_wait",
        JobState::Leased => "leased",
        JobState::Succeeded => "succeeded",
        JobState::DeadLettered => "dead_lettered",
        JobState::Cancelled => "cancelled",
    }
}

fn parse_state(value: &str) -> Result<JobState, StoreError> {
    match value {
        "pending" => Ok(JobState::Pending),
        "ready" => Ok(JobState::Ready),
        "retry_wait" => Ok(JobState::RetryWait),
        "leased" => Ok(JobState::Leased),
        "succeeded" => Ok(JobState::Succeeded),
        "dead_lettered" => Ok(JobState::DeadLettered),
        "cancelled" => Ok(JobState::Cancelled),
        _ => Err(StoreError::CorruptData),
    }
}

fn unix_millis(value: OffsetDateTime) -> i64 {
    i64::try_from(value.unix_timestamp_nanos() / 1_000_000).unwrap_or_else(|_| {
        if value.unix_timestamp_nanos().is_negative() {
            i64::MIN
        } else {
            i64::MAX
        }
    })
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("resource not found")]
    NotFound,
    #[error("lease compare-and-set failed")]
    LeaseConflict,
    #[error("invalid store configuration or request")]
    InvalidConfiguration,
    #[error("database contains an unknown state")]
    CorruptData,
}
