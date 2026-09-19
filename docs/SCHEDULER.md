# Scheduler Semantics

## Graph validation

Kahn's algorithm builds indegrees and outgoing adjacency once. Each vertex enters the queue once and each edge is examined once, so time and memory are O(V + E). Stable sorting of newly available IDs makes validation output deterministic.

## Ready ordering

The executable reference scheduler uses a binary heap ordered by availability time, descending priority, DAG ID, and job ID. Insertion and claim are O(log n); stale heap entries are discarded by comparing the row state and availability timestamp.

PostgreSQL uses the same logical order and a partial B-tree index. `SKIP LOCKED` allows multiple workers to seek distinct rows without waiting behind the first claimant.

## Lease invariant

A result is accepted only if:

1. the job is still `leased`;
2. the owner matches;
3. the UUID token matches; and
4. the lease deadline is still in the future.

This is a compare-and-set boundary. At-least-once execution remains possible around worker/network failure, but stale workers cannot commit after ownership changes. Real side effects should therefore also be idempotent or use an external idempotency key.

## Retry policy

Attempt count increments at claim. A failure or expiry uses `min(60s, 1s × 2^(attempt-1))`. The in-memory model adds stable, bounded jitter for deterministic tests. After `max_attempts`, the job becomes `dead_lettered`; dependents remain blocked for operator review.

## Cancellation

Cancellation changes every non-terminal job to `cancelled`, clears lease fields, and marks the DAG cancelled. Terminal success/dead-letter evidence remains. A late heartbeat or completion fails its predicate.

## Admission and boundedness

The reference scheduler accepts at most 100 DAGs and 10,000 ready jobs by default. Each DAG has at most 1,000 jobs, payload/result fields are at most 4 KiB, priority is -100..100, and attempts are 1..20. Deployed admission quotas should additionally be scoped per authenticated tenant.

