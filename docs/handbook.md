# Cloud Job Orchestrator handbook

By Jason Stys.

Preview validated DAG structure, inspect coherent job/event snapshots and review bounded unmet-parent, retry and lease evidence. Added real PostgreSQL API/worker execution and static-site delivery checks.

Turns workflow state into actionable diagnosis while keeping planning read-only. Inspection reports omit payloads, results and lease tokens; deployment still supplies a trusted identity/network/TLS boundary.

## Read the documentation

| Edition                                          | Audience and contents                                                                       |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------- |
| [project-guide.docx](reports/project-guide.docx) | User and developer guide: purpose, current usage, design, boundaries and source references. |

The editable Markdown remains the maintenance source. The Word files are readable editions, and the workbook is an evidence index rather than a test runner. See [publication identities and review](reports/publications.json) and the [documentation refresh record](reports/documentation-refresh-2026-11-09.md).

## Current usage and help

[README quick start](../README.md) is the entry point. Use the project-specific guides below before changing inputs, operating modes or deployment assumptions.

### Usage and operations

- [DISASTER RECOVERY](DISASTER_RECOVERY.md)
- [OPERATIONS](OPERATIONS.md)
- [WORKFLOW INSPECTION](WORKFLOW_INSPECTION.md)

### Design and contracts

- [API](API.md)
- [ARCHITECTURE](ARCHITECTURE.md)
- [DATA MODEL](DATA_MODEL.md)
- [RESEARCH](RESEARCH.md)
- [adr/0001 postgres leases](adr/0001-postgres-leases.md)

### Security and validation

- [SECURITY](SECURITY.md)
- [TESTING](TESTING.md)

## Historical results and reproduction

The reviewed source is [`c93ca92be7ffa6bdd4e44f0773cae5146a57ecf4`](https://github.com/JasonStys/cloud-job-orchestrator/tree/c93ca92be7ffa6bdd4e44f0773cae5146a57ecf4). The observed functional workflow baseline is `c93ca92be7ffa6bdd4e44f0773cae5146a57ecf4`; those observations are recorded at `2026-11-10T05:25:40.628667+00:00`. They do not certify an unrun later revision.

- [Detailed recorded validation](reports/PRACTICAL_WORKFLOWS_20261109.md)
- [Current Actions](https://github.com/JasonStys/cloud-job-orchestrator/actions) records later integration checks separately.
- Preserve dated measurements and follow the existing testing guide to reproduce them; a test-source inventory alone does not establish execution.

## Practical boundaries

- Trusted reference control plane; authentication, authorization, tenant separation and TLS are deployment responsibilities, not provided hosted services.
- Workers execute only bounded delay, SHA-256 checksum and uppercase operations; this is not a general remote command runner.
- Dependency waves are structural, not duration, concurrency or fairness forecasts. Captured countdowns are not live and do not reap leases.
- Inspection omits payloads, results and lease tokens, but other existing endpoints expose job results; inspection redaction is not access control.

## Maintaining these editions

Update the editable usage/design/test documents first. Rebuild and visually review every Word page and workbook tab before replacing an edition; verify the exact binary digests and evidence source in publications.json. Keep account data, logs with secrets, installed-app state and private test stores out of documentation. Source and release assets retain their existing security and licensing boundaries.
