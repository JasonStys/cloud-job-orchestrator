# Plan and inspect a workflow

Created by Jason Stys. Operators can now distinguish dependency waits, retry
delays, expired-but-not-reaped leases and terminal upstream blockers. One
read-only PostgreSQL `REPEATABLE READ` transaction captures jobs, edges, its
database clock and the newest 200 recorded attempt events. Refresh deliberately
requests a new snapshot. Reading does not claim, heartbeat, retry, cancel or reap.

```text
cargo run --locked --bin orchestratorctl -- plan examples/release-dag.json
cargo run --locked --bin orchestratorctl -- inspect release_demo --database-url <disposable-database-url>
```

`plan` needs no database and executes no task. It validates the original bounded
graph, then lists each job's sorted direct parents and earliest dependency-only
wave. Independent jobs can share a wave. These are structural dependencies, not
concurrency commitments, critical-path durations, deadline forecasts or fairness
guarantees. Unknown/duplicate JSON fields, duplicate edges, cycles, bad endpoints,
identifiers and payload/policy limits are rejected before submission. CLI reads
are capped at 1 MB from an ordinary explicitly selected file.

The same plan is available through `POST /v1/plans` and the dashboard's editable
JSON preview. It never submits a DAG. Edits invalidate the displayed plan and
discard late responses. The existing safe delay/checksum/uppercase operation
allowlist remains the only executable work; no shell, dynamic plugin or arbitrary
command was introduced.

`GET /v1/dags/{id}/inspection` and the CLI return:

| Field | Meaning |
|---|---|
| `snapshot` | Captured job state/attempts/lease metadata; application results are omitted. |
| `observed_at_ms` | Database transaction time used for all countdowns. Values do not tick live. |
| `dependencies` | Declared direct edges from the same captured snapshot. |
| `unmet_parents` | Direct parents that were not succeeded in this snapshot. |
| `terminal_blockers` | Distinct dead-lettered/cancelled ancestors along unsatisfied dependency paths. They explain inability to release work, not the cause of an ancestor's failure. |
| `terminal_blocker_count`, `terminal_blockers_truncated` | Exact count plus explicit bounded-list status. At most 16 ancestor IDs per job and 1,000 per report are listed. Terminal jobs no longer await dependency release. A zero-length truncated list never means no blockers. |
| `retry_wait_remaining_ms` | Nonnegative remaining availability delay, only for retry-wait jobs. Zero does not override dependencies or grant a worker lease. |
| `lease_remaining_ms` | Nonnegative lease remainder, only for leased jobs with a deadline. |
| `lease_expired_at_snapshot` | Inclusive deadline comparison. It does not imply recovery already ran. |
| `events` | Newest-first sequence/job/type/attempt/time metadata. Sequences are decimal strings to preserve full database integer precision in JavaScript. No lease token, application payload/result or arbitrary event detail is exported. |
| `events_truncated` | Older history exists beyond the 200-row window. An empty window does not prove no work occurred. |

The dashboard preserves the selected workflow when refreshing, offers bounded
25-item keyset pages and prevents old responses from replacing a newer selection.
Empty/error navigation clears stale exports. Downloaded JSON is the exact captured
inspection shown, with no second inconsistent query. Counts, explanations and
events share one snapshot. Keyboard-accessible scroll regions, table headings,
status messages and responsive layout support desktop/mobile use. An exactly full
final page can offer one empty next page; Previous workflows returns safely.

Build `npm --prefix web run build` before starting the service from the repository
root. Both local execution and Docker serve **`web/dist`**. The obsolete root
`nest_service` mount was corrected to Axum's supported `fallback_service` after a
real router test exposed a startup panic. No runtime warning was suppressed.

Bounds remain 1,000 jobs and 10,000 dependencies. Structural planning uses
O((V+E) log V) time; explaining all ancestor sets has a conservative
O(V × (V+E) log V) bound, with O(V+E) working graph memory plus the bounded
reported ancestor sets. This small reference service prioritizes clear evidence
over claiming large-scale graph analytics. Event reads use the existing
`(dag_id, sequence DESC)` index and fetch 201 rows to detect truncation.

Deploy only behind your own trusted identity/network/TLS boundary, as in the
original project. This update does not supply multi-tenant authentication or
production deployment. Reports remain operational metadata and should be stored
privately. Snapshot isolation follows [PostgreSQL's transaction
contract](https://www.postgresql.org/docs/current/sql-set-transaction.html);
the evidence cannot promise a job's future state after refresh.

Verification separates browser API-contract fixtures from genuine backend
evidence. The original PostgreSQL CI now also starts its own compiled API and
worker, serves the built dashboard, previews without submission, completes a
three-step DAG, checks exact uppercase/SHA-256/delay results and verifies real
captured event/metadata output. It uses only a disposable loopback
`orchestrator_test` database and terminates only its own child processes.

Portable `cargo test --all-targets --locked` explicitly reports the four database
tests as ignored. They no longer return success without performing assertions
when a database is missing. CI runs
`cargo test --locked --test postgres_integration -- --ignored --test-threads=1`;
missing/nonlocal/wrong-name database configuration fails rather than becoming a
pass. One native test concurrently reads 32 captured snapshots while 32 jobs are
claimed/completed, requiring exact job-state/event coherence in every snapshot.

The SQLx facade's lock resolution retained an unused vulnerable SQLite driver.
This PostgreSQL-only project now pins the same maintained `sqlx-core` and
`sqlx-postgres` 0.9.0 components directly behind a small import module; unrelated
drivers/macros are absent from the lockfile. SQL bytes remain embedded at compile
time through `include_str!`, and SQLx's `Migration::new`/`Migrator::with_migrations`
retain checksums, locking and transaction defaults. A portable test compares the
complete original file-source migration list, versions, types, descriptions,
bytes, checksums and transaction flags with the embedded list. Future component
upgrades must keep both exact versions synchronized and rerun that contract.
Compatible ctutils 0.4.4 and yoke-derive 0.8.4 replace advisory/yanked versions;
the full lockfile audit remains strict, with no ignored advisories.
