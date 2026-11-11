/** File: scripts/tests/rustsec-policy.test.mjs
 * Purpose: Verify the actual advisory workflow and negative mutations retain a read-only, fresh, failure-sensitive gate.
 * Functions: policy checks the deliberately reviewed final-job format, not arbitrary YAML or provider state.
 * State: no Actions mutation, source execution, token or advisory suppression is involved.
 */
import { readFileSync } from "node:fs";
import { test } from "node:test";
import assert from "node:assert/strict";
const workflow = readFileSync(
  new URL("../../.github/workflows/ci.yml", import.meta.url),
  "utf8",
);

/** Pin the literal security job recipe; trusted source review remains primary. */
function policy(source) {
  const sections = source.split("\n  rustsec:\n");
  assert.equal(sections.length, 2);
  const job = sections[1];
  assert.match(job, /timeout-minutes: 10/);
  assert.match(job, /permissions:\s+contents: read\s+steps:/);
  assert.doesNotMatch(
    job,
    /\bwrite\b|audit-check@|secrets\.|continue-on-error|--ignore|--no-fetch|--stale|\|\|/,
  );
  assert.equal(
    job.split("cargo install cargo-audit --version 0.23.0 --locked").length,
    2,
  );
  assert.equal(
    job.split(
      "cargo audit --file Cargo.lock --deny warnings --json | tee artifacts/rustsec-audit.json",
    ).length,
    2,
  );
  assert.match(job, /shell: bash[\s\S]*set -euo pipefail/);
}
test("actual advisory job uses the exact read-only failure-sensitive recipe", () =>
  policy(workflow));
for (const [name, changed] of [
  [
    "warnings become informational only",
    workflow.replace("--deny warnings", ""),
  ],
  [
    "grant unnecessary write",
    workflow.replace(
      "  rustsec:\n",
      "  rustsec:\n    permissions: {checks: write}\n",
    ),
  ],
  [
    "suppress a specific finding",
    workflow.replace(
      "--deny warnings",
      "--deny warnings --ignore RUSTSEC-2026-0486",
    ),
  ],
  [
    "disable current advisory fetch",
    workflow.replace("--deny warnings", "--deny warnings --no-fetch"),
  ],
  ["hide pipeline failure", workflow.replace("set -euo pipefail", "set -eu")],
])
  test("reject " + name, () => assert.throws(() => policy(changed)));
