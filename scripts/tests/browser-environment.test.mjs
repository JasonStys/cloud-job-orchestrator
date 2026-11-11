/** File: browser-environment.test.mjs; Purpose: Keep the actual three-engine/mobile suite on its exact nonroot official dependency image. Variables: real package lock, workflow and independent mutations; no browser result is mocked. */
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const workflow = readFileSync(new URL("../../.github/workflows/ci.yml", import.meta.url), "utf8").replaceAll("\r\n", "\n");
const lock = JSON.parse(readFileSync(new URL("../../web/package-lock.json", import.meta.url), "utf8"));
const configuration = readFileSync(new URL("../../web/playwright.config.ts", import.meta.url), "utf8");

/** Require the reviewed complete environment without dropping original engine/mobile assertions. */
function requireEnvironment(source) {
  const job = source.split("  web:\n")[1]?.split("  repository-and-scripts:\n")[0];
  assert.ok(job);
  const version = lock.packages["node_modules/playwright"].dependencies["playwright-core"];
  assert.ok(job.includes(`image: mcr.microsoft.com/playwright:v${version}-noble@sha256:2f05f8a734509a7b3f26a912639488ee08f58a7b292d14d62271bcff9038768f`));
  for (const value of ["options: --user 1001", "timeout-minutes: 10", "node scripts/check-browser-host.mjs chromium firefox webkit", "npm --prefix web run test:browser", "npm --prefix web run validate", "npm --prefix web run format:check", "npm --prefix web audit --audit-level=high"]) assert.ok(job.includes(value), `Missing original/host contract: ${value}`);
  assert.doesNotMatch(job, /apt-get|with-deps|install-deps|SKIP_VALIDATE_HOST_REQUIREMENTS|continue-on-error|\|\|\s*true/);
  for (const name of ["chromium", "firefox", "webkit", "mobile"]) assert.ok(configuration.includes(`name: "${name}"`));
}

test("actual browser job keeps matching exact vendor image, nonroot identity and original full suite", () => requireEnvironment(workflow));
for (const [label, before, after] of [
  ["moving image", "@sha256:2f05f8a734509a7b3f26a912639488ee08f58a7b292d14d62271bcff9038768f", ""],
  ["root identity", "--user 1001", "--user 0"],
  ["omitted engine", "chromium firefox webkit", "chromium firefox"],
  ["removed actual assertions", "npm --prefix web run test:browser", "true"],
  ["ignored actual proof failure", "node scripts/check-browser-host.mjs chromium firefox webkit", "node scripts/check-browser-host.mjs chromium firefox webkit || true"],
]) test(`reject ${label}`, () => assert.throws(() => requireEnvironment(workflow.replaceAll(before, after))));
