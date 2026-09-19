# ADR 0001: Coordinate bounded jobs with PostgreSQL leases

- Status: Accepted
- Date: 2026-09-18

## Context

The project needs durable DAG dependencies, concurrent claims, retries, cancellation, event history, and a demonstrable recovery model without operating a separate broker. At-least-once delivery is acceptable; stale completions are not.

## Decision

Use PostgreSQL as both durable state store and coordination boundary. Workers claim with a short row-locking transaction using `FOR UPDATE SKIP LOCKED`, then release the lock while retaining logical ownership in an owner/token/deadline lease tuple. Completion, heartbeat, failure, and cancellation are compare-and-set updates.

## Alternatives

- A message broker improves high-throughput delivery and partitioning but still needs a relational DAG/source-of-truth model and introduces reconciliation between systems.
- Holding database locks for the duration of work simplifies ownership but creates long transactions, vacuum pressure, connection exhaustion, and poor crash behavior.
- In-memory scheduling is fast but cannot coordinate replicas or survive process loss; it remains only as the executable specification.

## Consequences

Correctness is concentrated in reviewable SQL transactions and constraints, deployment has one stateful dependency, and recovery is easy to demonstrate. PostgreSQL becomes the scaling boundary, workers must make side effects idempotent, and operators must monitor database contention and queue age. A later broker integration should use an outbox and keep PostgreSQL authoritative.

