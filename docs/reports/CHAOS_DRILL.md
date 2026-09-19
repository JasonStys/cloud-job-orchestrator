# Worker-Loss Drill

## Scenario

A worker claims a job with a 20 ms lease and then stops before completing. After the deadline, the reaper moves the job to `retry_wait` and clears all lease fields. The original owner then attempts to commit with its old token.

## Assertion

The old completion affects zero rows and returns `LeaseConflict`; the snapshot remains `retry_wait`. This is implemented in both the in-memory state-machine test and `tests/postgres_integration.rs` against a real PostgreSQL service.

## Operational extension

For a deployment drill, submit `examples/release-dag.json`, stop the worker pod after a claim log, wait one lease period, restart workers, and verify a new token/attempt completes the graph. Capture queue age, expiry count, and total recovery time.

