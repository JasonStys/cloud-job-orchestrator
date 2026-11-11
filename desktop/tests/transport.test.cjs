/** File: desktop/tests/transport.test.cjs
 * Purpose: Verify actual owned loopback transport ignores inherited proxies and refuses false success.
 * Functions: inert local HTTP fixtures cover body limits/redirects/deadlines and exact route/source data.
 * State: no credentials, external server, ordinary profile or database is used.
 */
"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const http = require("node:http");
const { once } = require("node:events");
const { requestLocal, MAX_BODY } = require("../runtime.cjs");
const TOKEN = "a".repeat(64);

async function server(handler) {
  const value = http.createServer(handler);
  value.listen(0, "127.0.0.1");
  await once(value, "listening");
  return value;
}
async function close(value) {
  await new Promise((resolve, reject) =>
    value.close((error) => (error ? reject(error) : resolve())),
  );
}

test("real local socket retains exact headers/raw JSON and never visits inherited proxy", async () => {
  let proxyRequests = 0,
    observed;
  const proxy = await server((_request, response) => {
    proxyRequests++;
    response.end('{"proxy":true}');
  });
  const target = await server((request, response) => {
    let body = "";
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      observed = {
        path: request.url,
        method: request.method,
        token: request.headers["x-cjo-native"],
        origin: request.headers.origin,
        body,
      };
      response.writeHead(201, { "Content-Type": "application/json" });
      response.end('{"actual":true}');
    });
  });
  const names = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "NODE_USE_ENV_PROXY",
    "NO_PROXY",
    "no_proxy",
  ];
  const old = Object.fromEntries(
    names.map((name) => [name, process.env[name]]),
  );
  try {
    for (const name of names.slice(0, 4))
      process.env[name] = `http://127.0.0.1:${proxy.address().port}`;
    process.env.NODE_USE_ENV_PROXY = "1";
    process.env.NO_PROXY = "";
    process.env.no_proxy = "";
    const body = '{"id":"first","id":"second","jobs":[]}';
    assert.deepEqual(
      await requestLocal(target.address().port, TOKEN, {
        route: "/v1/dags",
        method: "POST",
        body,
      }),
      { actual: true },
    );
    assert.deepEqual(observed, {
      path: "/v1/dags",
      method: "POST",
      token: TOKEN,
      origin: `http://127.0.0.1:${target.address().port}`,
      body,
    });
    assert.equal(proxyRequests, 0);
  } finally {
    for (const name of names) {
      if (old[name] === undefined) delete process.env[name];
      else process.env[name] = old[name];
    }
    await close(target);
    await close(proxy);
  }
});
for (const status of [302, 403, 500]) {
  test(
    "real HTTP" + status + " cannot be promoted to successful JSON",
    async () => {
      const target = await server((_request, response) => {
        response.writeHead(status, { Location: "https://foreign.invalid/" });
        response.end('{"success":true}');
      });
      try {
        await assert.rejects(
          requestLocal(target.address().port, TOKEN, {
            route: "/native/status",
            method: "GET",
          }),
          /Operation refused/,
        );
      } finally {
        await close(target);
      }
    },
  );
}
test("actual oversized response and held response fail under finite transport bounds", async () => {
  const oversized = await server((_request, response) =>
    response.end("x".repeat(MAX_BODY + 1)),
  );
  try {
    await assert.rejects(
      requestLocal(oversized.address().port, TOKEN, {
        route: "/native/status",
        method: "GET",
      }),
      /exceeds|before completion/,
    );
  } finally {
    await close(oversized);
  }
  const held = await server((_request, _response) => {});
  try {
    await assert.rejects(
      requestLocal(
        held.address().port,
        TOKEN,
        { route: "/native/status", method: "GET" },
        40,
      ),
      /deadline/,
    );
  } finally {
    await close(held);
  }
});
