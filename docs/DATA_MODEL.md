# Data Model

## `dags`

One row per submitted graph. `state` is `running` or `cancelled`. Timestamps support retention planning and auditing.

## `jobs`

Composite primary key `(dag_id, id)`. The row stores the fixed kind, bounded payload, attempt policy, priority, state, next availability, lease tuple, result, and update timestamp. A check constraint enforces that all three lease fields are present exactly when state is `leased`.

## `job_dependencies`

Composite edge key `(dag_id, parent_id, child_id)` with foreign keys to both jobs. Self-edges are rejected in SQL; cycles are rejected by application validation before insertion.

## `job_events`

Identity-ordered append-only claim and terminal-transition evidence. An index on `(dag_id, sequence DESC)` supports recent history retrieval when an event API is added.

## Index rationale

- `jobs_claimable_idx` is partial on `ready`/`retry_wait` and ordered by availability, priority, and stable identifiers.
- `jobs_lease_expiry_idx` is partial on `leased`, making reaping proportional to live leases rather than total history.
- `dependency_child_idx` supports unmet-parent checks and child release.
- Pagination uses the DAG primary key as an opaque cursor and never performs increasing `OFFSET` scans.

## Retention

The schema intentionally does not delete history automatically. Operators should export audit data, then delete terminal DAGs older than their policy in bounded batches. Cascading foreign keys remove jobs, edges, and events together. Backup and restore validation must precede enabling a scheduled retention job.

