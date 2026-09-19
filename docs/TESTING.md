# Testing Strategy

## Risk matrix

| Risk | Test layer | Gate |
|---|---|---|
| Cycles or missing dependencies | Unit + generated property tests | Rust portable |
| Wrong ready ordering | Deterministic scheduler unit tests | Rust portable |
| Duplicate concurrent claims | 16-worker real-database test | PostgreSQL integration |
| Late worker overwrites recovery | Unit + real-database expiry/CAS test | Rust + PostgreSQL |
| Retry/dead-letter boundary | State-machine unit tests | Rust portable |
| Unsafe worker behavior | Allowlist unit tests + repository policy | Rust + policy |
| Dashboard type/view regressions | Strict compiler + Node tests + production build | Web |
| Unsafe restore target | Bats fail-closed tests | Scripts |
| Weak deployment defaults | Static policy + Terraform validate + Compose parse | Infrastructure |
| Packaging drift | Locked non-root image build | Container |
| Vulnerable dependencies/code | npm audit + RustSec + CodeQL | Security |

## Local commands

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
npm --prefix web ci --ignore-scripts
npm --prefix web run validate
npm run code-index:check
npm run validate
```

PostgreSQL tests skip locally only when `DATABASE_URL` is absent. If the variable is present, connection or migration failure fails the test. GitHub CI always provides PostgreSQL 18 and therefore cannot silently skip integration coverage.

## Fault injection

`expiry_requeues_and_rejects_a_late_worker` claims a short lease, allows its worker to “die” by sleeping past the deadline, reaps it, then verifies the original token cannot complete. This isolates the central crash-recovery invariant without timing a full deployment.

## Performance method

Algorithmic complexity is asserted by design and small deterministic simulations. This repository does not publish synthetic throughput claims as production capacity. Before deployment, run a workload against production-equivalent PostgreSQL, capture p50/p95/p99 claim/complete latency, lock wait time, database CPU/IO, queue age, and successful jobs/second, then establish a load-shedding threshold below saturation.

