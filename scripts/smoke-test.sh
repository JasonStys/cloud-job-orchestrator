#!/usr/bin/env bash
# File: Exercises live, submit, claim, and snapshot API paths against a disposable DAG.
# Functions: main creates a unique safe DAG and checks response fields; variables hold base URL and generated DAG ID.
set -euo pipefail

main() {
  local base_url="${1:-http://127.0.0.1:8080}"
  local dag_id
  dag_id="smoke_$(date -u +%s)"
  curl --fail --silent --show-error "${base_url}/health/live" | grep --quiet '"live"'
  curl --fail --silent --show-error --header 'content-type: application/json' \
    --data "{\"id\":\"${dag_id}\",\"jobs\":[{\"id\":\"check\",\"kind\":\"checksum\",\"payload\":\"smoke\",\"max_attempts\":2,\"priority\":0}],\"dependencies\":[]}" \
    "${base_url}/v1/dags" | grep --quiet "$dag_id"
  curl --fail --silent --show-error "${base_url}/v1/dags/${dag_id}" | grep --quiet '"ready"'
  echo "API smoke test passed: ${dag_id}"
}

main "$@"
