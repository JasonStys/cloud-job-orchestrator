# Architecture

## Goals

The system accepts bounded synthetic DAGs, runs each node after all parents succeed, tolerates worker loss, prevents late workers from overwriting newer outcomes, and provides enough operational surface to diagnose and recover it. Correctness under concurrent claims is more important than maximum throughput.

## Components

1. `orchestratord` exposes the versioned HTTP API, health probes, metrics, and compiled dashboard.
2. `PostgresStore` owns durable state changes. Every multi-row transition uses a database transaction.
3. `orchestrator-worker` claims one job at a time and executes only a compile-time operation allowlist.
4. `orchestratorctl` validates specs offline and provides direct administrative commands.
5. PostgreSQL holds DAGs, jobs, dependency edges, lease metadata, and append-only job events.
6. The browser dashboard is a read-only operations view using safe DOM text insertion.

## Data flow

1. Submission validates size, identifiers, payloads, attempt budgets, priorities, edge endpoints, and acyclicity before beginning database writes.
2. A transaction inserts the DAG, every job, and every dependency. Zero-parent jobs begin as `ready`.
3. A worker claim first reaps expired leases, then locks one eligible row with `FOR UPDATE SKIP LOCKED` and changes it to `leased` with a random token and expiry.
4. Completion requires the exact DAG, job, owner, token, leased state, and unexpired deadline. A successful compare-and-set releases children only if every parent has succeeded.
5. Failure or expiry either sets `retry_wait` with bounded exponential delay or `dead_lettered` when the attempt budget is exhausted.

## Scaling model

API replicas are stateless. Workers scale horizontally because row-level locks serialize claims. PostgreSQL is the single source of truth and therefore the first capacity boundary. The claim index orders available work by priority and availability without scanning terminal jobs. Keyset pagination avoids offset cost growth.

## Failure boundaries

- A process crash before commit rolls back the transaction.
- A worker crash after claim leaves a lease that the next claim/reaper recovers.
- A delayed worker cannot commit after lease expiry because the completion predicate includes token, owner, state, and deadline.
- A database outage makes readiness fail and state-changing requests return an internal/unavailable response; it does not fall back to divergent memory state.
- A partial graph submission cannot occur because graph insertion is atomic.

## Observability

HTTP request spans and service events are emitted as structured JSON with a propagated `x-request-id`. Health endpoints separate process liveness from database readiness. `/metrics` emits Prometheus text. Kubernetes stdout collection or an OpenTelemetry Collector can transform/route structured traces and logs; the service does not silently drop to an in-process telemetry buffer.

## Deployment

The release image runs as UID/GID 10001 with a read-only filesystem in Kubernetes. Control-plane and worker deployments scale separately. A disruption budget protects API availability; an HPA bounds workers from one to twenty replicas. The example network policy is intentionally default-deny and must be extended with explicit API ingress, DNS, and database egress rules for the target cluster.

