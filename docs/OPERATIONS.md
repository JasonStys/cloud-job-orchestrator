# Operations Runbook

## Signals

- Liveness: `GET /health/live` indicates the process event loop can respond.
- Readiness: `GET /health/ready` executes `SELECT 1`; remove the pod from traffic when it fails.
- Metrics: scrape `/metrics`; production expansion should add queue age, claims, lease expiries, retry count, dead letters, and transition latency.
- Logs: collect JSON stdout and correlate on `x-request-id`, DAG ID, and job ID.

## Common incidents

### Database unavailable

Confirm readiness failures across replicas, database service health, credentials/secret version, network policy, connection saturation, storage, and failover state. Do not restart every API and worker simultaneously. Restore database reachability, verify migrations, then bring worker concurrency up gradually.

### Rising lease expiry

Compare job duration with lease duration, worker CPU/memory throttling, shutdown behavior, and database latency. Because bundled jobs are bounded, repeated expiry usually indicates infrastructure rather than work complexity. Reduce worker replicas if database contention is the cause; increase lease duration only after measuring safe completion time.

### Dead-lettered job

Read the DAG snapshot and recent structured logs. Correct the underlying input or worker defect. This version preserves dead letters but deliberately provides no “force retry” endpoint; resubmit a new DAG with a new ID so the audit trail remains explicit.

## Rollout

1. Back up and verify checksum.
2. Run migrations against staging and execute the smoke test.
3. Deploy one control-plane canary; verify readiness and error/latency signals.
4. Roll out API replicas, then workers in small batches.
5. Submit the release demo and observe dependency order.

## Rollback

The initial migration is additive. Roll back application Deployments to the prior immutable image tag while leaving schema data in place. Do not manually reverse a migration during an incident. For a future incompatible schema, use expand/migrate/contract across separate releases and document the minimum compatible version.

