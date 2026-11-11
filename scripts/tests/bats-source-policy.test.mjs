/** File: bats-source-policy.test.mjs; Purpose: Keep the actual original recovery tests on exact official Bats source without mutable global setup. Variables: actual workflow text and independent negative mutations; no network or test-result mocking. */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const workflow = readFileSync(new URL("../../.github/workflows/ci.yml", import.meta.url), "utf8");

/** Check the narrow reviewed job recipe, not a general YAML security parser. */
function requireBatsRecipe(source) {
  const jobs = [...source.matchAll(/^  repository-and-scripts:\s*\r?\n([\s\S]*?)(?=^  [a-z][\w-]*:|$(?![\s\S]))/gm)];
  assert.equal(jobs.length, 1);
  const body = jobs[0][1];
  for (const required of ["repository: bats-core/bats-core", "ref: f7defb94362f2053a3e73d13086a167448ea9133", "path: .runtime/bats-core", "persist-credentials: false", 'test "$(.runtime/bats-core/bin/bats --version)" = \'Bats 1.10.0\'', "run: .runtime/bats-core/bin/bats scripts/tests/recovery.bats", "run: shellcheck scripts/backup.sh scripts/restore-drill.sh scripts/smoke-test.sh", "timeout-minutes: 10"]) assert.ok(body.includes(required), `Missing original/pinned recipe: ${required}`);
  assert.doesNotMatch(body, /apt-get|continue-on-error|\|\|\s*true/);
}

test("actual recovery job retains exact official source/version and the original assertions", () => requireBatsRecipe(workflow));
for (const [label, before, after] of [
  ["mutable Bats ref", "ref: f7defb94362f2053a3e73d13086a167448ea9133", "ref: master"],
  ["foreign repository", "repository: bats-core/bats-core", "repository: other/project"],
  ["unverified version", 'test "$(.runtime/bats-core/bin/bats --version)"', 'echo "unverified"'],
  ["removed original cases", "run: .runtime/bats-core/bin/bats scripts/tests/recovery.bats", "run: true"],
  ["increased original deadline", "timeout-minutes: 10\n    steps:", "timeout-minutes: 90\n    steps:"],
]) test(`reject ${label}`, () => assert.throws(() => requireBatsRecipe(workflow.replaceAll(before, after))));
