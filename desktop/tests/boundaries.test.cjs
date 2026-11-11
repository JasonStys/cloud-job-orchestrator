/** File: desktop/tests/boundaries.test.cjs
 * Purpose: Verify fixed RPC admission and genuine bounded local file/no-overwrite boundaries.
 * Functions: tests use only new temporary inert fixtures; no application/real profile is opened.
 * State: independent malformed-route/body/alias sentinels, not implementation-derived worker oracles.
 */
"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { join } = require("node:path");
const { tmpdir } = require("node:os");
const { validateRequest, MAX_BODY } = require("../runtime.cjs");
const { readRegular, readUtf8, newFile, ancestors } = require("../files.cjs");

for (const route of [
  "/native/status",
  "/v1/dags?limit=25",
  "/v1/dags?limit=25&after=release_2",
  "/v1/dags/mission-1",
  "/v1/dags/mission-1/inspection",
]) {
  test("admit fixed native read " + route, () =>
    assert.equal(validateRequest({ route, method: "GET" }).route, route),
  );
}
for (const route of [
  "/v1/plans",
  "/v1/dags",
  "/native/workers",
  "/native/reap",
  "/v1/dags/mission-1/cancel",
]) {
  test("admit fixed object write " + route, () =>
    assert.equal(
      validateRequest({ route, method: "POST", body: { paused: true } })
        .serialized,
      '{"paused":true}',
    ),
  );
}
for (const route of [
  "https://foreign.invalid/",
  "//foreign.invalid/",
  "/v1/leases/claim",
  "/v1/dags/../secret",
  "/v1/dags/%2e%2e",
  "/v1/dags/a?extra=true",
  "/native/status#fragment",
  "/v1/dags?limit=1000",
  "/v1/dags?limit=25&after=",
]) {
  test("refuse non-contract route " + route, () =>
    assert.throws(() => validateRequest({ route, method: "GET" })),
  );
}
for (const body of [null, 1, [], "null", "[]", "1", "{invalid"]) {
  test("refuse non-object write " + String(body), () =>
    assert.throws(() =>
      validateRequest({ route: "/v1/dags", method: "POST", body }),
    ),
  );
}
test("preserve exact source JSON for original Rust duplicate-field rejection", () => {
  const body = '{"id":"first","id":"second","jobs":[]}';
  assert.equal(
    validateRequest({ route: "/v1/dags", method: "POST", body }).serialized,
    body,
  );
});
test("bound actual UTF8 input and reject extra/mutating read fields", () => {
  assert.throws(() =>
    validateRequest({
      route: "/v1/dags",
      method: "POST",
      body: { value: "🚀".repeat(MAX_BODY / 4) },
    }),
  );
  assert.throws(() =>
    validateRequest({ route: "/native/status", method: "GET", body: {} }),
  );
  assert.throws(() =>
    validateRequest({
      route: "/native/status",
      method: "GET",
      endpoint: "foreign",
    }),
  );
  assert.throws(() =>
    validateRequest({ route: "/native/status", method: "DELETE" }),
  );
});
test("actual regular bytes are bounded and malformed kinds refuse before mutation", () => {
  const root = fs.mkdtempSync(join(tmpdir(), "cjo-file-read-")),
    file = join(root, "fixture.json");
  fs.writeFileSync(file, "12345", { flag: "wx" });
  assert.equal(readRegular(file, 5).toString(), "12345");
  assert.throws(() => readRegular(file, 4));
  assert.throws(() => readRegular(root, 100));
  const alias = join(root, "linked.json");
  fs.linkSync(file, alias);
  assert.throws(() => readRegular(alias, 100));
  assert.equal(fs.readFileSync(file, "utf8"), "12345");
});
test("actual new-file exports preserve existing files/hardlinks and flush exact content", () => {
  const root = fs.mkdtempSync(join(tmpdir(), "cjo-file-write-")),
    file = join(root, "capture.json"),
    alias = join(root, "alias.json");
  newFile(file, '{"value":42}\n');
  fs.linkSync(file, alias);
  assert.throws(() => newFile(file, "changed"));
  assert.throws(() => newFile(alias, "changed"));
  assert.equal(fs.readFileSync(file, "utf8"), '{"value":42}\n');
  assert.equal(fs.readFileSync(alias, "utf8"), '{"value":42}\n');
  assert.throws(() => ancestors(file));
});

test("actual malformed UTF8 is refused without silently changing document bytes", () => {
  const root = fs.mkdtempSync(join(tmpdir(), "cjo-utf8-read-")),
    file = join(root, "input.json");
  const invalid = Buffer.from([123, 34, 120, 34, 58, 34, 255, 34, 125]);
  fs.writeFileSync(file, invalid, { flag: "wx" });
  assert.throws(() => readUtf8(file, 100));
  assert.deepEqual(fs.readFileSync(file), invalid);
  fs.writeFileSync(file, Buffer.from('\ufeff{"x":"telemetry"}', "utf8"));
  assert.equal(readUtf8(file, 100), '{"x":"telemetry"}');
});
