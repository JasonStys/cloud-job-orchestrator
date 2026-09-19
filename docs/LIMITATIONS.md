# Limitations and Next Steps

- No authentication, authorization, or tenant isolation is implemented; external exposure is unsafe until those controls exist.
- Worker actions are deliberately synthetic. Adding business work requires an explicit typed operation, idempotency design, payload schema, and targeted tests—not an arbitrary command field.
- The worker does not heartbeat during an operation because every bundled operation is shorter than the default lease. Longer typed operations require a cancellable heartbeat task.
- PostgreSQL is a single coordination dependency. Multi-region active/active semantics, partition tolerance, and regional data placement are outside this version.
- Retry jitter exists in the reference scheduler but the SQL retry expression is deterministic. Production fleets should add bounded randomized jitter to reduce synchronized retries.
- `/metrics` exposes readiness only. Queue/transition metrics and an authenticated observability boundary are a next increment.
- Event history is written for claims and terminal worker transitions but is not yet available through the API.
- The default-deny network policy needs cluster-specific allow rules before deployment.
- Terraform intentionally provisions only safe namespace/config scaffolding; database, secret, ingress, and image registry modules vary by cloud and organization.
- There is no automated retention job. Add one only after backup/restore drills are measured and accepted.

