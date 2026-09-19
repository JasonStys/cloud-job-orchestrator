# Validation Report

Generated for release `1.0.0` on 2026-09-18. The committed report describes the deterministic gate contract and links to the current remote evidence.

Remote evidence: [CI workflow](https://github.com/JasonStys/cloud-job-orchestrator/actions/workflows/ci.yml) and [CodeQL workflow](https://github.com/JasonStys/cloud-job-orchestrator/actions/workflows/codeql.yml).

| Gate | Expected evidence | Local result |
|---|---|---|
| Rust formatting | `cargo fmt --all -- --check` | Pass |
| Rust static analysis | all targets, locked dependencies, warnings denied | Pass |
| Rust test suite | 8 unit + 3 scheduler/property tests; DB tests skip only without URL | Pass |
| Example validation | deterministic topological order | Pass |
| Dashboard | strict TypeScript, 2 Node tests, production build | Pass |
| npm audit | locked dependency audit at high severity | Pass: 0 vulnerabilities |
| Repository policy | headers, docs, exact code index, SHA-pinned actions, neutral content | Pending final document index |
| Infrastructure policy | non-root, read-only root, limits, probes, PDB, HPA, network denial | Pass |
| PostgreSQL integration | migration, 16 concurrent unique claims, expiry/CAS rejection | CI-only |
| Bash recovery | syntax, ShellCheck, Bats fail-closed behavior | CI-only |
| Terraform | recursive format, init without backend, validate | Pass with Terraform 1.14.6; CI pins current 1.16.3 |
| Container | locked multi-stage non-root build | CI-only |
| RustSec | 230 locked crate dependencies against 1,251 advisories | Pass: 0 vulnerabilities |
| CodeQL | Rust and TypeScript security-and-quality queries | CI-only |

The local Docker daemon and PostgreSQL client/server are unavailable in the authoring environment, so those gates are intentionally delegated to Linux GitHub-hosted runners rather than claimed as local evidence.
