#!/usr/bin/env bash
# File: Creates a compressed, checksummed PostgreSQL backup without embedding credentials.
# Functions: usage and main validate inputs before invoking pg_dump; variables describe source URL and output directory.
set -euo pipefail

usage() { echo "usage: backup.sh DATABASE_URL OUTPUT_DIRECTORY"; }

main() {
  if [[ $# -ne 2 || -z "$1" || -z "$2" ]]; then usage >&2; return 64; fi
  local database_url="$1"
  local output_directory="$2"
  mkdir -p -- "$output_directory"
  local timestamp backup_path
  timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
  backup_path="${output_directory%/}/orchestrator-${timestamp}.dump"
  pg_dump --format=custom --compress=9 --no-owner --no-privileges --file="$backup_path" "$database_url"
  sha256sum "$backup_path" > "${backup_path}.sha256"
  printf 'backup=%s\nchecksum=%s\n' "$backup_path" "${backup_path}.sha256"
}

main "$@"

