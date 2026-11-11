# Native Cloud Job Orchestrator

Created by Jason Stys · application 1.1.0 · Windows x64 candidate.

The full application combines the existing Rust/Axum control plane, real PostgreSQL 18.6 and the
TypeScript operations dashboard. Its bundled desktop shell supplies the runtime and owns the local
database and two workers. Prepared scenarios are editable data inside this application.

## Open and use the application

Download the complete Windows archive and checksum from an accepted release, verify the checksum,
extract the whole folder and open `CloudJobOrchestrator.exe`. Keep adjacent files together. The
packaged app needs no installed Rust, Node, PostgreSQL, IDE, Docker, administrator service or command
setup. It remains unsigned, with Windows/OS-library dependencies; this is not a clean-machine or
SmartScreen certification. Source builds and releases are separate from installation.

1. The native startup window explains local database preparation; Help and safe Quit remain available.
2. Choose prepared Release verification, Telemetry processing, Timed dependency or Unicode output
   constraint input. You can edit the complete workflow JSON or explicitly import a regular JSON file.
   A prepared case does not submit anything. Use a new workflow ID when reusing a case.
3. Preview the dependency plan, then explicitly Submit. Original validation limits still apply:
   identifiers of 1–64 ASCII letters/digits/underscore/hyphen, 1–1,000 jobs, at most 10,000 edges,
   acyclic dependencies, bounded attempts/priorities and a 4,096-byte payload per job. The editor caps
   one document at 1 MB. Native requests preserve raw JSON so Rust can reject duplicate fields.
4. Start workers explicitly. They run only Delay (0–5,000 ms), SHA-256 Checksum and Uppercase. These
   are genuine original worker operations, never arbitrary programs or cloud tasks. Refresh to obtain
   a new captured observation. Pause lets current jobs finish; it does not revoke their leases.
5. Select a workflow, navigate pages, inspect job states/results, dependency blockers, retry/lease
   observations and the newest 200 attempt events. Countdown fields refer to their captured instant,
   not a live clock. Missing older events do not prove no work occurred. The Unicode example exercises
   a real output-size failure and leaves its dependent blocked; no successful result is manufactured.
6. Cancel selected work only after the confirmation. Original compare-and-set checks reject revoked
   leases. Recover expired leases invokes the real recovery transaction, rather than editing a view.
7. Export captured results or download the operational inspection through an explicit new-file dialog.
   Results include private job output; operational inspection intentionally omits it. They are separate
   captured requests, not one guaranteed atomic joint snapshot. Existing destinations are refused.
8. Use Quit application, Application → Quit after saved work, or the window close control. Admission
   stops, retained HTTP/worker work finishes, the pool closes and the owned PostgreSQL cluster stops
   before ownership is released. A finite test deadline is not a universal shutdown-time promise.

## Local data and recovery

Default durable data is below `%LOCALAPPDATA%/JasonStysPortfolio/cloud-job-orchestrator/workspace`.
An explicit `--profile ABSOLUTE_PATH` selects an empty or already identified trusted workspace.
Application/install/resource folders, existing aliases, hardlinked mutable files and unknown nonempty
directories are refused before profile initialization. Normal closed database recovery files remain.
The private database password is generated per profile, never shipped in source, URLs or renderer data.

Only open profiles you own and trust. A real PostgreSQL profile contains configuration and database
objects, not merely inert imported JSON. Do not open downloaded or attacker-supplied clusters. This is
one trusted OS-owner application, not a shared enterprise authorization boundary. Files are plaintext
and sensitive; checksums detect accidental changes but do not authenticate a malicious same-owner writer.

Close the application fully before copying the complete profile for backup. Captured JSON exports
are not database backups. Partial initialization, incompatible versions, corruption and an existing
PostgreSQL owner marker are preserved for review, never silently reset, adopted or killed. The closed
profile admission guard has 10,000-member, 1 GiB/member and 4 GiB total bounds, plus a 4 MiB startup-log
bound. These are admission limits, not continuous disk quotas or space reservations. No automatic
history deletion, cloud upload, telemetry service or system PostgreSQL installation is added.

## Implementation and files

| File | Responsibility |
| --- | --- |
| `src/native_profile.rs` | Real OS ownership, private identified state, bounded descriptor reads, closed PostgreSQL tree and complete installation-folder admission. |
| `src/native_database.rs` | Exact provider/runtime resource verification, genuine isolated cluster lifecycle and authenticated cluster/version/canonical-directory identity. |
| `src/native_runtime.rs` | Two original retained workers, pause/stop observations and completion-aware API/pool shutdown. |
| `src/native_http.rs` | Exact loopback Host/Origin/capability boundary, bounded requests and native worker/recovery controls. |
| `src/bin/orchestrator_native.rs` | Compose the real database, original migrations, API and workers without changing deployed database configuration. |
| `desktop/main.cjs` | Visible startup/Help/Quit surface, sandboxed owned window and fixed sender-checked IPC/file dialogs. |
| `desktop/runtime.cjs` | Private stdin capability, source-bound readiness and bounded direct-only local HTTP; no inherited proxy or arbitrary lease/command route. |
| `desktop/preload.cjs` | Narrow frozen renderer bridge; no token, filesystem or Node access is exposed. |
| `desktop/files.cjs` | Bounded regular file reads and new-file-only flushed exports. |
| `web/src/api.ts`, `web/src/app.ts` | Preserve the normal website API; add actual native authoring, worker controls, result capture and prepared inputs only in the bundled app. |
| `scripts/postgres-sdk.py`, `scripts/check-crt.ps1` | Fixed official archive SHA/size and bounded selected extraction plus licensed Microsoft-signature-checked runtime inputs. |
| `scripts/build-native.mjs` | Coherent source metadata, original Rust build, full public runtime/notices and distinct native icon; fresh output only. |
| `scripts/archive_native.py` | Bounded complete Windows ZIP, safe relocation and actual extracted-app acceptance. |
| `scripts/native-core-smoke.py` | Independent real database/worker output, boundary, shutdown and restart oracles. |
| `scripts/test-native-app.mjs` | Real visible packaged controls, import/export, invalid input, direct-proxy proof, retained work and durable restart. |
| `desktop/tests/*.test.cjs`, `tests/test_native_archive.py` | Actual local file/HTTP/ZIP boundary regressions, separate from the original scientific/domain tests. |
| `.github/workflows/native-app.yml` | Actual Windows package consumer; bounded proof by default, full payload only trusted-main manual opt-in. |

The normal cloud services, original transactional lease semantics, deployment configuration and
backup/recovery tools remain available. The native app uses its own database rather than replacing
PostgreSQL with an in-memory or SQLite substitute. Declaration locations remain in `CODE_INDEX.md`.

## Build and validation scope

Build-time requirements are separate: Rust 1.98.1 with a licensed MSVC toolchain, current Node 24,
locked desktop/dashboard dependencies, build-only CPython 3.14.10 and the exact official PostgreSQL
18.6 archive. Signed app-local Microsoft runtime files require their separate redistribution terms.
The package includes actual PostgreSQL, Rust dependency/standard-library and Electron/Chromium notices.

`scripts/postgres-sdk.py --output .runtime/NEW_SDK --crt-sdk LICENSED_CRT_DIRECTORY` verifies the
provider input. Set `CJO_POSTGRES_SDK` to that directory and `CJO_NATIVE_OUTPUT=release/NEW_OUTPUT`,
then run `node scripts/build-native.mjs`. Run `python scripts/archive_native.py
release/NEW_OUTPUT/win-unpacked artifacts/native/NEW_PROOF` for complete relocation and real UI use.
Use fresh outputs; earlier failures, profiles and packages are preserved. Hosted source must be clean
and equal `GITHUB_SHA`. Local candidates disclose `sourceDirty`.

The native test uses only an exact marked synthetic profile and system-only executable PATH. It
forces an inert proxy configuration and requires the browser and private RPC to stay direct. It uses
real widgets, database transactions and worker results. A leased delayed job must finish normally
before Quit returns, while an unclaimed dependent stays genuinely ready after restart. Original
Linux/Windows tests, real PostgreSQL integration, browser contracts, infrastructure/container checks,
dependency audits and semantic CodeQL remain separate required hosted gates. Their results are not
inferred from a successful local GUI journey.

References: [PostgreSQL Windows binaries](https://www.postgresql.org/download/windows/),
[PostgreSQL cluster-name bound](https://www.postgresql.org/docs/18/runtime-config-logging.html#GUC-CLUSTER-NAME),
[Node direct HTTP agents](https://nodejs.org/docs/latest-v24.x/api/http.html),
[Microsoft runtime redistribution](https://learn.microsoft.com/en-us/visualstudio/releases/2026/redistribution),
[Visual Studio terms](https://visualstudio.microsoft.com/license-terms/vs2026-ga-community/).
