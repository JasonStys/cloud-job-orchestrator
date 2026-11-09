/** File: Jason Stys: actual owned API/static/dashboard/worker/PostgreSQL acceptance on a disposable CI database.
 * Functions: start captures bounded diagnostics privately; request/poll exercise real public interfaces.
 * Variables: children contains only processes spawned here; no user service is reused or terminated. */
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { once } from "node:events";
import { resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const database = new URL(
  process.env.DATABASE_URL ?? "postgres://invalid/invalid",
);
if (
  !["127.0.0.1", "localhost"].includes(database.hostname) ||
  database.pathname !== "/orchestrator_test"
)
  throw new Error(
    "Explicit disposable loopback orchestrator_test database required",
  );
const port = Number(process.env.CJO_LIVE_TEST_PORT ?? 18935);
if (!Number.isSafeInteger(port) || port < 1024 || port > 65535)
  throw new Error("Invalid owned test port");
const base = `http://127.0.0.1:${port}`;
const children = [];

function start(binary, env = {}) {
  const child = spawn(
    resolve(
      `target/debug/${binary}${process.platform === "win32" ? ".exe" : ""}`,
    ),
    [],
    { env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] },
  );
  const state = { child, output: "", closed: false, error: null };
  for (const stream of [child.stdout, child.stderr])
    stream.on("data", (chunk) => {
      state.output = (state.output + chunk.toString()).slice(-8192);
    });
  child.on("error", () => {
    state.error = "Owned test process could not launch";
  });
  child.on("close", () => {
    state.closed = true;
  });
  children.push(state);
  return state;
}

async function request(path, body) {
  const response = await fetch(`${base}${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: body === undefined ? {} : { "content-type": "application/json" },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    signal: AbortSignal.timeout(3000),
  });
  assert(
    response.ok,
    `Unexpected HTTP ${response.status} on owned acceptance path`,
  );
  return response.json();
}

async function poll(check, description) {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    for (const owned of children)
      assert(
        !owned.closed && owned.error === null,
        "Owned process exited before acceptance completed",
      );
    if (await check()) return;
    await delay(50);
  }
  throw new Error(`Timed out: ${description}`);
}

try {
  const api = start("orchestratord", { BIND_ADDRESS: `127.0.0.1:${port}` });
  // Only the newly spawned process can supply this readiness marker; never reuse an unrelated listener.
  await poll(
    () => api.output.includes("control plane is listening"),
    "owned API readiness",
  );
  const home = await fetch(base, { signal: AbortSignal.timeout(3000) });
  assert.equal(home.status, 200);
  assert(
    (await home.text()).includes("Created by"),
    "Compiled dashboard not served",
  );
  const js = await fetch(`${base}/app.js`, {
    signal: AbortSignal.timeout(3000),
  });
  assert.equal(js.status, 200);
  assert((await js.text()).includes("fetchInspection"));
  const id = `live_${randomUUID().replaceAll("-", "")}`;
  const spec = {
    id,
    jobs: [
      { id: "prepare", kind: "uppercase", payload: "report" },
      { id: "verify", kind: "checksum", payload: "artifact" },
      { id: "pause", kind: "delay", payload: "25" },
    ],
    dependencies: [
      { from: "prepare", to: "verify" },
      { from: "verify", to: "pause" },
    ],
  };
  const before = await request("/v1/dags?limit=100");
  const plan = await request("/v1/plans", spec);
  assert.deepEqual(
    plan.jobs.map((row) => [row.id, row.dependency_wave]),
    [
      ["prepare", 0],
      ["verify", 1],
      ["pause", 2],
    ],
  );
  assert.deepEqual(
    await request("/v1/dags?limit=100"),
    before,
    "Read-only plan mutated workflow listing",
  );
  await request("/v1/dags", spec);
  const initial = await request(`/v1/dags/${id}/inspection`);
  assert.deepEqual(
    initial.explanations.find((row) => row.id === "pause").unmet_parents,
    ["verify"],
  );
  start("orchestrator-worker", {
    WORKER_ID: "owned-acceptance-worker",
    POLL_MS: "10",
  });
  let final;
  await poll(async () => {
    final = await request(`/v1/dags/${id}`);
    return final.jobs.every((job) => job.state === "succeeded");
  }, "real bounded worker completion");
  assert.equal(final.jobs.find((job) => job.id === "prepare").result, "REPORT");
  assert.equal(
    final.jobs.find((job) => job.id === "verify").result,
    createHash("sha256").update("artifact").digest("hex"),
  );
  assert.equal(
    final.jobs.find((job) => job.id === "pause").result,
    "slept:25ms",
  );
  const captured = await request(`/v1/dags/${id}/inspection`);
  assert(captured.snapshot.jobs.every((job) => job.result === null));
  assert(
    captured.explanations.every(
      (row) =>
        row.unmet_parents.length === 0 && row.terminal_blockers.length === 0,
    ),
  );
  assert.equal(captured.events.length, 6);
  assert.equal(captured.events_truncated, false);
  assert(
    captured.events.every(
      (event) =>
        typeof event.sequence === "string" && !Object.hasOwn(event, "detail"),
    ),
  );
  console.log(
    "Actual compiled dashboard/API/plan/three-operation worker/captured PostgreSQL evidence: PASS",
  );
} finally {
  await Promise.all(
    children.map(async ({ child, closed }) => {
      if (closed) return;
      const ended = once(child, "close");
      child.kill("SIGTERM");
      await Promise.race([ended, delay(2000)]);
      if (child.exitCode === null && child.signalCode === null)
        child.kill("SIGKILL");
    }),
  );
}
