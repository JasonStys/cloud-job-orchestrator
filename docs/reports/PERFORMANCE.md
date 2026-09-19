# Performance Model

## Complexity

| Operation | Reference scheduler | Durable store |
|---|---:|---|
| DAG validation | O(V + E) | Performed before transaction |
| Ready insertion/claim | O(log n) | Partial B-tree seek plus one row lock |
| Dependency release | O(outdegree) | Indexed child lookup and unmet-parent anti-join |
| Snapshot | O(V log V) for stable in-memory sort | Primary-key ordered scan |
| DAG list page | O(log n + page) | Primary-key keyset page, maximum 100 |
| Lease reaping | O(live leased jobs) | Partial expiry-index scan |

## Guardrails

The API caps a DAG at 1,000 jobs, a page at 100 IDs, payload/result fields at 4 KiB, attempts at 20, leases at 60 seconds, and database connections at 100. Kubernetes examples impose CPU/memory limits and a worker HPA maximum of 20.

## Measurement protocol

No unrepeatable laptop throughput number is presented as capacity. A deployment benchmark should warm the database, run at least five minutes per concurrency level, preserve the graph/payload distribution, and record jobs/second, queue-age p50/p95/p99, claim/complete p95/p99, lease expiry, retry rate, PostgreSQL lock wait, CPU, IO, and buffer-cache hit rate. Increase load until the first service objective is violated, then set admission/load-shedding below that point with safety margin.

