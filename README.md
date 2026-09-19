# Cloud Job Orchestrator

A production-minded control plane for safe directed-acyclic-graph (DAG) workloads. Rust services validate dependencies, PostgreSQL coordinates concurrent workers with expiring compare-and-set leases, a dependency-free TypeScript dashboard exposes state, and Bash/Terraform/Kubernetes assets cover recovery and deployment.

The project demonstrates backend and cloud engineering without exposing a remote command runner. Workers can execute only three bounded synthetic operations: delay (maximum five seconds), SHA-256 checksum, and uppercase transformation.

## What it demonstrates

- O(V + E) cycle detection and deterministic topological ordering.
- O(log n) in-memory priority scheduling for the executable reference model.
- Transactional PostgreSQL claims using `FOR UPDATE SKIP LOCKED`.
- Lease heartbeats, expiry recovery, stale-worker rejection, retries, dead-lettering, and cancellation.
- Concurrent worker tests against a real PostgreSQL 18 service in CI.
- Keyset pagination, bounded payloads, bounded connection pools, and admission limits.
- Structured JSON logs, request IDs, health probes, and Prometheus-compatible readiness output.
- Non-root containers, restricted Kubernetes pods, resource limits, disruption budgets, autoscaling, network denial, and version-constrained Terraform.
- Backup, checksum, guarded restore, and disaster-recovery drill procedures.

## Architecture

```mermaid
flowchart LR
    U[CLI / Dashboard / API client] -->|validated DAG| A[Axum control plane]
    A -->|atomic graph transaction| P[(PostgreSQL)]
    W1[Bounded worker] -->|claim + heartbeat + CAS result| P
    W2[Bounded worker] -->|claim + heartbeat + CAS result| P
    P -->|state snapshots| A
    A -->|JSON / metrics / traces| O[Operations tooling]
```

The in-memory `Scheduler` is an executable specification used for fast property and state-machine tests. The deployed services use `PostgresStore`, where the database is the concurrency authority. See [Architecture](docs/ARCHITECTURE.md), [Scheduler semantics](docs/SCHEDULER.md), and [ADR 0001](docs/adr/0001-postgres-leases.md).

## Quick start

Prerequisites: Rust 1.98.1, Node.js 24, npm 11, and Docker Compose.

```bash
cargo test --all-targets --locked
npm --prefix web ci --ignore-scripts
npm --prefix web run validate
docker compose up --build --scale worker=2
```

In another terminal:

```bash
cargo run --locked --bin orchestratorctl -- validate examples/release-dag.json
DATABASE_URL=postgres://orchestrator:local-development-only@127.0.0.1/orchestrator \
  cargo run --locked --bin orchestratorctl -- submit examples/release-dag.json
```

Open `http://127.0.0.1:8080` for the operations dashboard. The Compose credentials are intentionally local-only; deployments inject `DATABASE_URL` through a secret.

## API summary

| Endpoint | Purpose |
|---|---|
| `GET /health/live` | Process liveness. |
| `GET /health/ready` | Database readiness. |
| `GET /metrics` | Prometheus-compatible readiness metric. |
| `POST /v1/dags` | Validate and atomically submit a DAG. |
| `GET /v1/dags` | List DAG IDs using a bounded keyset page. |
| `GET /v1/dags/{id}` | Read a stable job-state snapshot. |
| `POST /v1/dags/{id}/cancel` | Cancel unfinished jobs and revoke leases. |
| `POST /v1/leases/claim` | Claim one eligible job. |
| `POST /v1/leases/heartbeat` | Extend a live owner/token lease. |
| `POST /v1/leases/complete` | Commit a bounded result and release children. |
| `POST /v1/leases/fail` | Retry with backoff or dead-letter. |

The full request, response, and error contract is in [API](docs/API.md).

## Repository map

| Path | Responsibility |
|---|---|
| `src/domain.rs` | Serializable DAG types, identifier/payload policy, and O(V + E) validation. |
| `src/scheduler.rs` | Deterministic heap scheduler and lease state-machine reference implementation. |
| `src/store.rs` | PostgreSQL migrations, transactional claims, CAS heartbeats/results, recovery, and pagination. |
| `src/http.rs` | Versioned Axum routes, error mapping, request IDs, tracing, and same-origin dashboard serving. |
| `src/worker.rs` | Fixed safe-operation executor; no shell or arbitrary process capability. |
| `src/bin/orchestratord.rs` | Control-plane process entry point. |
| `src/bin/orchestrator_worker.rs` | Durable polling worker entry point. |
| `src/bin/orchestratorctl.rs` | Validation, submission, status, cancellation, and reaping CLI. |
| `migrations/` | Relational state machine, constraints, append-only events, and claim/expiry indexes. |
| `tests/` | Property, state-machine, PostgreSQL concurrency, and lease-expiry tests. |
| `web/` | Strict TypeScript dashboard, accessible HTML/CSS, and Node tests. |
| `scripts/` | Backup, guarded restore, smoke test, documentation index, and policy validation. |
| `infra/k8s/` | Restricted workloads, service, probes, resource limits, PDB, HPA, and network policy. |
| `infra/terraform/` | Version-pinned namespace/configuration module with validated variables. |
| `.github/workflows/` | Cross-platform, integration, infrastructure, container, audit, and CodeQL gates. |
| `docs/` | Architecture, API, schema, security, test, operations, recovery, limitations, ADR, and evidence. |

Every source file begins with a purpose/function/variable header. [The generated code index](docs/CODE_INDEX.md) records exact declaration line locations and CI fails when it is stale.

## Quality gates

Run the portable gates:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo run --locked --bin orchestratorctl -- validate examples/release-dag.json
npm --prefix web run validate
npm run code-index:check
npm run validate
```

CI additionally runs PostgreSQL integration tests, Bats and ShellCheck, Terraform validation, Docker Compose parsing, a non-root container build, dependency audits, and CodeQL for Rust and TypeScript. See [Testing](docs/TESTING.md) and [validation evidence](docs/reports/VALIDATION.md).

## Safety and scope

This is a portfolio-grade reference control plane, not a hosted multi-tenant job service. Authentication, authorization, TLS termination, secret delivery, externally managed PostgreSQL, and production telemetry backends are deployment responsibilities. The explicit non-goals and scale boundaries are documented in [Limitations](docs/LIMITATIONS.md) and [Security](docs/SECURITY.md).

## License

MIT. See [LICENSE](LICENSE).
