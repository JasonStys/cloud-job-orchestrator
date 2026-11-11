/** File: desktop/runtime.cjs
 * Purpose: Own the real native PostgreSQL/API/worker process and bounded exact-origin RPC.
 * Functions: startNative validates child readiness; request limits routes/data; close retains completion.
 * State: the private capability is sent by stdin, never argv/environment/renderer/URL or log data.
 */
"use strict";
const { spawn } = require("node:child_process");
const http = require("node:http");
const { randomBytes } = require("node:crypto");
const { join } = require("node:path");

const VERSION = "1.1.0";
const MAX_BODY = 1_048_576;

/** Keep renderer RPC within real documented operator actions; no lease impersonation or command text. */
function validateRequest(value) {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    Object.keys(value).some((key) => !["route", "method", "body"].includes(key))
  )
    throw new Error("Invalid operation");
  const { route, method, body } = value;
  const read =
    method === "GET" &&
    (route === "/native/status" ||
      /^\/v1\/dags\?limit=25(?:&after=[A-Za-z0-9_-]{1,64})?$/.test(route) ||
      /^\/v1\/dags\/[A-Za-z0-9_-]{1,64}(?:\/inspection)?$/.test(route));
  const write =
    method === "POST" &&
    (["/v1/plans", "/v1/dags", "/native/workers", "/native/reap"].includes(
      route,
    ) ||
      /^\/v1\/dags\/[A-Za-z0-9_-]{1,64}\/cancel$/.test(route));
  if (
    typeof route !== "string" ||
    (!read && !write) ||
    (read && body !== undefined)
  )
    throw new Error("Operation is outside the native contract");
  const serialized =
    body === undefined
      ? undefined
      : typeof body === "string"
        ? body
        : JSON.stringify(body);
  if (serialized && Buffer.byteLength(serialized) > MAX_BODY)
    throw new Error("Workflow exceeds 1 MiB");
  const parsed = typeof body === "string" ? JSON.parse(body) : body;
  if (write && (!parsed || typeof parsed !== "object" || Array.isArray(parsed)))
    throw new Error("An explicit JSON object is required");
  return { route, method, serialized };
}

/** Own one direct loopback socket: no inherited proxy/global agent, redirect or unbounded body. */
function requestLocal(port, capability, value, timeout = 12_000) {
  if (
    !Number.isInteger(port) ||
    port < 1 ||
    port > 65535 ||
    !/^[0-9a-f]{64}$/.test(capability) ||
    !Number.isInteger(timeout) ||
    timeout < 1 ||
    timeout > 12_000
  )
    throw new Error("Invalid owned local request identity");
  const { route, method, serialized } = validateRequest(value);
  const agent = new http.Agent({ keepAlive: false, proxyEnv: {} });
  return new Promise((resolve, reject) => {
    let finished = false,
      timer;
    function finish(error, data) {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      agent.destroy();
      if (error) reject(error);
      else resolve(data);
    }
    const request = http.request(
      {
        protocol: "http:",
        hostname: "127.0.0.1",
        port,
        path: route,
        method,
        agent,
        headers: {
          "x-cjo-native": capability,
          Origin: `http://127.0.0.1:${port}`,
          "Content-Type": "application/json",
        },
      },
      (response) => {
        let total = 0;
        const chunks = [];
        response.on("data", (chunk) => {
          total += chunk.length;
          if (total > MAX_BODY) {
            request.destroy(new Error("Native response exceeds 1 MiB"));
            return;
          }
          chunks.push(chunk);
        });
        response.once("error", (error) => finish(error));
        response.once("aborted", () =>
          finish(new Error("Native response ended before completion")),
        );
        response.once("end", () => {
          if (response.statusCode < 200 || response.statusCode >= 300) {
            finish(
              new Error(
                `Operation refused (HTTP ${response.statusCode}); refresh retained state before retry.`,
              ),
            );
            return;
          }
          try {
            finish(null, JSON.parse(Buffer.concat(chunks).toString("utf8")));
          } catch (error) {
            finish(error);
          }
        });
      },
    );
    request.once("error", (error) => finish(error));
    timer = setTimeout(
      () => request.destroy(new Error("Owned local request deadline exceeded")),
      timeout,
    );
    request.end(serialized);
  });
}

/** Start exactly one supplied owned binary; an unrelated listening process can never satisfy readiness. */
async function startNative({ binary, resources, profile, metadata }) {
  const capability = randomBytes(32).toString("hex");
  const environment = Object.fromEntries(
    Object.entries(process.env).filter(([key]) =>
      ["SYSTEMROOT", "WINDIR", "TEMP", "TMP"].includes(key.toUpperCase()),
    ),
  );
  environment.PATH = join(process.env.SYSTEMROOT ?? "C:/Windows", "System32");
  const child = spawn(
    binary,
    [
      "--profile",
      profile,
      "--postgres",
      join(resources, "postgres"),
      "--assets",
      resources,
    ],
    { env: environment, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] },
  );
  let closed = false,
    stopping = false,
    active = 0,
    ready,
    output = "",
    diagnostics = "";
  const pending = new Set();
  const completion = new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", (code, signal) => {
      closed = true;
      if (code === 0 && signal === null) resolve();
      else
        reject(
          new Error(
            "The owned database/service did not finish normally. Preserve its local profile.",
          ),
        );
    });
  });
  completion.catch(() => {}); // The retained owner reads this same completion; no unhandled-event leak.
  child.stderr.on("data", (block) => {
    diagnostics = (diagnostics + block.toString("utf8")).slice(-4096);
  });
  const identity = await new Promise((resolve, reject) => {
    const deadline = setTimeout(
      () =>
        reject(
          new Error(
            "Owned PostgreSQL startup did not become ready. Preserve the profile.",
          ),
        ),
      45_000,
    );
    const end = (error) => {
      clearTimeout(deadline);
      reject(error);
    };
    child.once("error", end);
    child.once("close", () =>
      end(
        new Error(
          "Owned PostgreSQL startup failed; no profile reset was attempted.",
        ),
      ),
    );
    child.stdout.on("data", (block) => {
      if (ready) return;
      output += block.toString("utf8");
      if (Buffer.byteLength(output) > 4096) {
        end(new Error("Owned readiness exceeded its bound"));
        return;
      }
      const newline = output.indexOf("\n");
      if (newline < 0) return;
      try {
        const value = JSON.parse(output.slice(0, newline));
        if (
          value.product !== "cloud-job-orchestrator" ||
          value.version !== VERSION ||
          value.source !== metadata.source ||
          value.source_dirty !== metadata.sourceDirty ||
          !Number.isInteger(value.port) ||
          value.port < 1 ||
          value.port > 65535 ||
          !/^[0-9a-f]{64}$/.test(value.instance)
        )
          throw new Error("Owned native identity differs from this package");
        ready = true;
        clearTimeout(deadline);
        resolve(value);
      } catch (error) {
        end(error);
      }
    });
    child.stdin.write(capability + "\n");
  }).catch(async (error) => {
    child.stdin.end();
    await completion.catch(() => {});
    throw error;
  });
  const origin = `http://127.0.0.1:${identity.port}`;

  async function request(value) {
    if (stopping || closed)
      throw new Error("The owner is closing; no new work can start.");
    const { route, method, serialized } = validateRequest(value);
    const operation = (async () => {
      active += 1;
      try {
        return await requestLocal(identity.port, capability, {
          route,
          method,
          body: serialized,
        });
      } finally {
        active -= 1;
      }
    })();
    pending.add(operation);
    try {
      return await operation;
    } finally {
      pending.delete(operation);
    }
  }
  let closing;
  function close() {
    if (!closing)
      closing = (async () => {
        stopping = true;
        await Promise.allSettled([...pending]);
        child.stdin.end();
        await completion; // No timer, signal or guessed zero substitutes for PostgreSQL/worker completion.
      })();
    return closing;
  }
  return {
    origin,
    identity,
    child,
    request,
    close,
    completion,
    get activeRequests() {
      return active;
    },
    get diagnostics() {
      return diagnostics;
    },
  };
}

module.exports = {
  VERSION,
  MAX_BODY,
  validateRequest,
  requestLocal,
  startNative,
};
