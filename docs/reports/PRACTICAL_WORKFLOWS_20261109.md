# Planning and captured workflow validation — 2026-11-09

Created by Jason Stys. Local Windows x64, exact Rust 1.98.1, Node 24. Synthetic
owned data only. Hosted PostgreSQL, Linux infrastructure/container and CodeQL
acceptance must run against the committed head; none is inferred from mocks.

The CLI/API now preview dependency waves without submitting work. Operators get
one captured job/dependency/time/event view with exact unmet parents, terminal
blocker counts, bounded lists, retry/lease values, explicit expired-not-reaped
state and event truncation. Dashboard selection/pagination, draft invalidation,
stale/error/empty protection and exact captured JSON downloads complete the
workflow. Authored values enter the DOM only through safe text APIs.

| Local check | Observation |
|---|---|
| Portable Rust suite | 20 native graph/state/property/route/CLI/migration-contract tests passed; four actual PostgreSQL tests explicitly ignored locally and mandatory via `--ignored` in CI. |
| Real browser journeys | 12 passed across desktop Chromium/Firefox/WebKit and mobile WebKit: selection/refresh/pages, expired leases, explicit event window, hostile literal text, exact JSON download, malformed/failed/edit-invalidated plans, keyboard use, empty/errors and late response ordering. Axe found no tested WCAG rule violations. |
| TypeScript | Strict type checks, two original Node aggregation checks and production build passed. |
| Native quality | Warning-denying Clippy, rustfmt, generated declaration index and repository/infrastructure policy checks passed. |
| Dependencies | Complete npm audit: 0 findings. Complete Cargo audit: 218 locked crates, no vulnerabilities or denied warnings after compatible patches and genuine removal of the unused driver graph. |

Independent review identified a report-amplification edge case: a valid 1,000-job
chain could repeat hundreds of terminal ancestors per row. The report now lists
at most 16 blocker IDs per job and 1,000 overall, retaining exact distinct counts
and explicit truncation. Terminal jobs are not awaiting release. Maximum-size
cancelled and partly failed chains verify counts, budgets and a serialized
explanation below 1 MB without changing the scheduler.

A real router test also exposed the pre-existing Axum root nesting startup panic;
the supported static fallback now serves the actual `web/dist` build locally and
in Docker. No assertion or runtime warning was hidden. PostgreSQL tests formerly
returned success immediately without `DATABASE_URL`; this false-pass behavior is
removed. The four explicit database tests reject absent/nonlocal/wrong-name
configuration and are required in CI, including concurrent state/event coherence
and the 201-row truncation boundary.

`scripts/verify-live.mjs` is an additional genuine hosted acceptance path: it
starts only its own compiled API and worker on disposable loopback PostgreSQL,
checks actual static/JS delivery, verifies that planning did not submit, executes
uppercase/SHA-256/delay steps and checks precise results, six real attempt events
and captured result redaction. It never reuses an existing process or touches
Docker configuration. Local browser contracts are clearly separate from this
backend evidence. The script has not been claimed as locally passed without its
database; its actual hosted result is the merge gate.

Bounds and remaining deployment/interpretation limits are documented in the
[workflow guide](../WORKFLOW_INSPECTION.md). This remains a trusted-control-plane
reference, not hosted multi-tenant accounts, operational duration forecasting or
universal production certification. Browser automation does not replace manual
assistive-technology testing. Existing historical validation reports remain
point-in-time records.
