#!/usr/bin/env bash
# File: Restores a backup only into an explicitly disposable drill database.
# Functions: usage and main enforce a `_drill` suffix before pg_restore; variables identify archive and target URL/name.
set -euo pipefail

usage() { echo "usage: restore-drill.sh BACKUP.dump TARGET_DATABASE_URL TARGET_DATABASE_NAME_drill"; }

main() {
  if [[ $# -ne 3 ]]; then usage >&2; return 64; fi
  local archive="$1"
  local target_url="$2"
  local target_name="$3"
  if [[ ! -f "$archive" ]]; then echo "backup archive does not exist" >&2; return 66; fi
  if [[ ! "$target_name" =~ ^[A-Za-z0-9_]+_drill$ ]]; then
    echo "refusing restore: target database name must end in _drill" >&2
    return 65
  fi
  pg_restore --exit-on-error --clean --if-exists --no-owner --no-privileges --dbname="$target_url" "$archive"
  psql "$target_url" --set=ON_ERROR_STOP=1 --tuples-only --command="SELECT count(*) FROM dags;"
  echo "restore drill passed for $target_name"
}

main "$@"

