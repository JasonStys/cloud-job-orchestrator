# API Contract

All bodies are JSON except `/metrics`. Identifiers use 1–64 ASCII letters, digits, `_`, or `-`. Request bodies retain Axum's default limit, while every payload/result is independently capped at 4,096 bytes.

## Submit a DAG

`POST /v1/dags`

```json
{
  "id": "release_demo",
  "jobs": [
    {"id": "prepare", "kind": "uppercase", "payload": "candidate", "max_attempts": 3, "priority": 10}
  ],
  "dependencies": []
}
```

Returns `201 {"dag_id":"release_demo"}`. Invalid DAGs return `422`; duplicate IDs return `409`.

## List and inspect

- `GET /v1/dags?after=<id>&limit=25` returns `items` and a keyset `next_cursor`. Limit range is 1–100.
- `GET /v1/dags/{id}` returns jobs ordered by ID with state, attempts, availability, lease metadata, and result.
- `POST /v1/dags/{id}/cancel` revokes unfinished leases and returns the count transitioned.

## Worker protocol

Claim:

```json
{"owner":"worker-a","lease_ms":15000}
```

`POST /v1/leases/claim` returns `{"job":null}` or a job with a UUID lease token. The owner is 1–64 bytes and lease duration is 1–60,000 ms.

Heartbeat and failure use:

```json
{
  "dag_id":"release_demo",
  "job_id":"prepare",
  "owner":"worker-a",
  "lease_token":"00000000-0000-0000-0000-000000000000",
  "lease_ms":15000
}
```

Completion replaces `lease_ms` with a bounded `result`. A stale, expired, cancelled, or mismatched lease returns `409 lease_conflict`.

## Error envelope

```json
{"code":"invalid_dag","message":"dependency graph contains a cycle"}
```

Stable codes are `invalid_request`, `invalid_dag`, `already_exists`, `not_found`, `lease_conflict`, `unavailable`, and `internal`. Database detail is logged but not returned.

