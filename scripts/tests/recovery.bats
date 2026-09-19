#!/usr/bin/env bats
# File: Bats tests prove recovery scripts fail closed before reaching database tools.

@test "backup rejects missing arguments" {
  run bash scripts/backup.sh
  [ "$status" -eq 64 ]
  [[ "$output" == *"usage:"* ]]
}

@test "restore refuses a non-drill database" {
  archive="$BATS_TEST_TMPDIR/empty.dump"
  touch "$archive"
  run bash scripts/restore-drill.sh "$archive" "postgres://invalid" "production"
  [ "$status" -eq 65 ]
  [[ "$output" == *"refusing restore"* ]]
}

