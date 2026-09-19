# Security

## Threat model

Untrusted actors may submit malformed graphs, oversized content, cycles, forged lease tokens, stale results, or high-volume requests. Workers and database connections may fail independently. Repository dependencies and CI actions may be compromised.

## Controls

- The worker operation is an enum; no shell, executable path, URL fetch, or arbitrary code field exists.
- Identifiers, job and dependency counts, payload/result size, priority, attempts, lease duration, connection count, and page size are bounded. Graph cardinality is rejected before graph working memory is allocated.
- DAGs are cycle-checked before one-transaction insertion.
- Lease tokens use UUID v4; every mutation checks owner, token, state, and expiry.
- SQL values are bound parameters. Table/state names are static.
- API errors do not expose database messages.
- Containers are non-root; Kubernetes disables privilege escalation, capabilities, writable root, and service-account token mounting.
- The example namespace enforces restricted pod security and includes a default-deny network policy.
- Actions use immutable full commit SHAs, dependencies are locked, RustSec/npm audits run, and CodeQL covers Rust and TypeScript.
- Restore tooling refuses targets whose explicit name does not end with `_drill`.

## Deployment requirements

Place the API behind authenticated TLS ingress. Map identity to tenant-aware authorization before accepting external submissions. Use a managed secret store for `DATABASE_URL`, a least-privilege database role, encrypted backups, explicit DNS/database network-policy egress, rate limits, request-body limits at ingress, and an external telemetry/alert pipeline.

## Reporting

Do not open a public issue for a suspected vulnerability. Use GitHub's private vulnerability reporting feature if enabled for the repository, or contact the repository owner privately through the profile contact channel.
