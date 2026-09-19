-- File: Creates the durable DAG, job, dependency, and append-only event model.
CREATE TABLE IF NOT EXISTS dags (
    id TEXT PRIMARY KEY CHECK (id ~ '^[A-Za-z0-9_-]{1,64}$'),
    state TEXT NOT NULL DEFAULT 'running' CHECK (state IN ('running', 'cancelled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS jobs (
    dag_id TEXT NOT NULL REFERENCES dags(id) ON DELETE CASCADE,
    id TEXT NOT NULL CHECK (id ~ '^[A-Za-z0-9_-]{1,64}$'),
    kind TEXT NOT NULL CHECK (kind IN ('delay', 'checksum', 'uppercase')),
    payload TEXT NOT NULL CHECK (octet_length(payload) <= 4096),
    max_attempts SMALLINT NOT NULL CHECK (max_attempts BETWEEN 1 AND 20),
    attempts SMALLINT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    priority SMALLINT NOT NULL CHECK (priority BETWEEN -100 AND 100),
    state TEXT NOT NULL CHECK (state IN ('pending', 'ready', 'retry_wait', 'leased', 'succeeded', 'dead_lettered', 'cancelled')),
    available_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    lease_owner TEXT,
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    result TEXT CHECK (octet_length(result) <= 4096),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (dag_id, id),
    CHECK ((state = 'leased') = (lease_owner IS NOT NULL AND lease_token IS NOT NULL AND lease_expires_at IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS job_dependencies (
    dag_id TEXT NOT NULL,
    parent_id TEXT NOT NULL,
    child_id TEXT NOT NULL,
    PRIMARY KEY (dag_id, parent_id, child_id),
    FOREIGN KEY (dag_id, parent_id) REFERENCES jobs(dag_id, id) ON DELETE CASCADE,
    FOREIGN KEY (dag_id, child_id) REFERENCES jobs(dag_id, id) ON DELETE CASCADE,
    CHECK (parent_id <> child_id)
);

CREATE TABLE IF NOT EXISTS job_events (
    sequence BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    dag_id TEXT NOT NULL,
    job_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    attempt SMALLINT NOT NULL,
    detail JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (dag_id, job_id) REFERENCES jobs(dag_id, id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS jobs_claimable_idx
    ON jobs (available_at, priority DESC, dag_id, id)
    WHERE state IN ('ready', 'retry_wait');
CREATE INDEX IF NOT EXISTS jobs_lease_expiry_idx
    ON jobs (lease_expires_at)
    WHERE state = 'leased';
CREATE INDEX IF NOT EXISTS dependency_child_idx
    ON job_dependencies (dag_id, child_id);
CREATE INDEX IF NOT EXISTS events_dag_sequence_idx
    ON job_events (dag_id, sequence DESC);

