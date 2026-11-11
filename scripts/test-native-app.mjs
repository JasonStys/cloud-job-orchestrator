/** File: scripts/test-native-app.mjs
 * Purpose: Use the actual bundled Electron/Rust/PostgreSQL application, not a source-server substitute.
 * Functions: launch and finish own one marked synthetic profile, real widgets, file IO and normal exits.
 * State: private acceptance artifacts only; no normal AppData, global request headers or Docker access.
 */
import {
  _electron as electron,
  expect,
} from "../desktop/node_modules/@playwright/test/index.mjs";
import { mkdir, mkdtemp, writeFile, readFile, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, dirname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { once } from "node:events";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import http from "node:http";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const candidate = resolve(
  root,
  process.env.CJO_NATIVE_OUTPUT ?? "release/pilot-002",
);
const executable = process.env.CJO_NATIVE_EXECUTABLE
  ? resolve(process.env.CJO_NATIVE_EXECUTABLE)
  : join(candidate, "win-unpacked/CloudJobOrchestrator.exe");
const output = resolve(
  root,
  process.env.CJO_NATIVE_PROOF ?? "artifacts/native/pilot-002",
);
if (!output.startsWith(join(root, "artifacts/native") + sep))
  throw new Error(
    "Fresh proof must stay under this project artifacts/native/.",
  );
await mkdir(dirname(output), { recursive: true });
await mkdir(output);
const scratch = await mkdtemp(join(tmpdir(), "cjo-native-consumer-"));
await writeFile(join(scratch, "acceptance.marker"), "cjo-owned-native-test\n", {
  flag: "wx",
  mode: 0o600,
});
const metadata = JSON.parse(
  await readFile(
    join(dirname(executable), "resources/native/SOURCE.json"),
    "utf8",
  ),
);
expect(metadata.version).toBe("1.1.0");
expect(metadata.source).toBe(
  execFileSync(
    "git",
    ["-c", `safe.directory=${root.replaceAll("\\", "/")}`, "rev-parse", "HEAD"],
    { cwd: root, encoding: "utf8" },
  ).trim(),
);
for (const name of ["main.cjs", "runtime.cjs", "preload.cjs", "files.cjs"]) {
  expect(metadata.asar["desktop/" + name]).toBe(
    createHash("sha256")
      .update(await readFile(join(root, "desktop", name)))
      .digest("hex"),
  );
}
expect(typeof metadata.sourceDirty).toBe("boolean");
if (process.env.CI) expect(metadata.sourceDirty).toBe(false);
const externalRequests = [],
  pageErrors = [];
const environment = Object.fromEntries(
  Object.entries(process.env).filter(([key]) =>
    ["SYSTEMROOT", "WINDIR", "TEMP", "TMP"].includes(key.toUpperCase()),
  ),
);
environment.PATH = join(process.env.SYSTEMROOT ?? "C:/Windows", "System32");
environment.CJO_NATIVE_ACCEPTANCE_ROOT = scratch;
let current;
let proxyRequests = 0;
const inertProxy = http.createServer((_request, response) => {
  proxyRequests++;
  response.writeHead(502);
  response.end("Unexpected proxy use refused");
});
inertProxy.listen(0, "127.0.0.1");
await once(inertProxy, "listening");

async function launch() {
  const application = await electron.launch({
    executablePath: executable,
    args: [
      `--proxy-server=http://127.0.0.1:${inertProxy.address().port}`,
      "--proxy-bypass-list=<-loopback>",
    ],
    env: environment,
    timeout: 60_000,
  });
  current = application;
  const page = await application.firstWindow({ timeout: 50_000 });
  await expect(page.locator("#native-startup-status")).toBeVisible({
    timeout: 5000,
  });
  expect(
    await application.evaluate(({ BrowserWindow }) =>
      BrowserWindow.getAllWindows()[0].isVisible(),
    ),
  ).toBe(true);
  await expect(page.locator("h1")).toHaveText("Job Control Plane", {
    timeout: 50_000,
  });
  const origin = new URL(page.url()).origin;
  expect(
    await application.evaluate(
      ({ BrowserWindow }, url) =>
        BrowserWindow.getAllWindows()[0].webContents.session.resolveProxy(url),
      origin,
    ),
  ).toBe("DIRECT");
  expect(new URL(page.url()).hostname).toBe("127.0.0.1");
  page.on("request", (request) => {
    if (new URL(request.url()).origin !== origin)
      externalRequests.push(request.url());
  });
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await expect(page.locator("#native-status")).toContainText("Version 1.1.0");
  return { application, page, origin };
}
async function finish(application, page) {
  const child = application.process(),
    exited = once(child, "exit");
  await page.locator("#native-quit").click();
  let timer;
  let result;
  try {
    result = await Promise.race([
      exited,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("Owned normal app exit did not complete")),
          45_000,
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
  expect(result).toEqual([0, null]);
  expect(
    await stat(join(scratch, "profile/postgres/postmaster.pid")).then(
      () => true,
      (error) => {
        if (error.code === "ENOENT") return false;
        throw error;
      },
    ),
  ).toBe(false);
  current = undefined;
}
async function showCompleted(page) {
  await expect
    .poll(
      async () => {
        await page.locator("#refresh").click();
        return (await page.locator("#jobs").innerText()).includes("succeeded");
      },
      { timeout: 15_000, intervals: [100, 200, 500] },
    )
    .toBe(true);
  await expect(page.locator("#jobs")).toContainText("RELEASE READY");
  await expect(page.locator("#jobs")).toContainText(
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  );
}

try {
  const first = await launch(),
    { page, application } = first;
  expect(
    await page.evaluate(async () => (await fetch("/native/status")).status),
  ).toBe(403);
  await page.locator("#native-prepare").click();
  await page.locator("#plan-form button").click();
  await expect(page.locator("#plan-rows li")).toHaveCount(2);
  await page.locator("#native-submit").click();
  await expect(page.locator("#dag-select")).toHaveValue("release-verification");
  await page.locator("#native-start").click();
  await showCompleted(page);
  const saved = join(scratch, "captured-results.json");
  await application.evaluate(({ dialog }, path) => {
    dialog.showSaveDialog = async () => ({ canceled: false, filePath: path });
  }, saved);
  await page.locator("#native-results").click();
  await expect
    .poll(async () =>
      readFile(saved, "utf8").then(
        (text) => JSON.parse(text).jobs.length,
        () => 0,
      ),
    )
    .toBe(2);
  const captured = JSON.parse(await readFile(saved, "utf8"));
  expect(captured.jobs.map((value) => value.result)).toEqual([
    "RELEASE READY",
    "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  ]);
  await page.locator("#native-results").click();
  await expect(page.locator("#native-status")).toContainText(
    "Destination already exists",
  );
  expect(JSON.parse(await readFile(saved, "utf8"))).toEqual(captured);
  const valid = await page.locator("#plan-input").inputValue();
  await page.locator("#plan-input").fill('{"id":"invalid","jobs":[]}');
  await page.locator("#native-submit").click();
  await expect(page.locator("#native-status")).toContainText("HTTP 422");
  await page
    .locator("#plan-input")
    .fill('{"id":"first","id":"second","jobs":[]}');
  await page.locator("#native-submit").click();
  await expect(page.locator("#native-status")).toContainText("HTTP 422");
  await expect(page.locator("#dag-select")).toHaveValue("release-verification");
  await page.locator("#plan-input").fill(valid);
  await page.locator("#refresh").click();
  await expect(page.locator("#status")).toContainText(
    "Showing release-verification",
  );
  await page.screenshot({
    path: join(output, "native-results.png"),
    fullPage: true,
  });
  await page.locator("#native-pause").click();
  const imported = join(scratch, "operator-workflow.json");
  await writeFile(
    imported,
    JSON.stringify({
      id: "imported-workflow",
      jobs: [
        { id: "review", kind: "uppercase", payload: "inspection accepted" },
      ],
    }),
    { flag: "wx" },
  );
  await application.evaluate(({ dialog }, path) => {
    dialog.showOpenDialog = async () => ({
      canceled: false,
      filePaths: [path],
    });
  }, imported);
  await page.locator("#native-import").click();
  await expect(page.locator("#plan-input")).toHaveValue(
    await readFile(imported, "utf8"),
  );
  await page.locator("#native-submit").click();
  await page.locator("#dag-select").selectOption("imported-workflow");
  page.once("dialog", (dialog) => dialog.accept());
  await page.locator("#native-cancel").click();
  await expect(page.locator("#jobs")).toContainText("cancelled");
  await page.locator("#scenario").selectOption("unicode");
  await page.locator("#native-prepare").click();
  await page.locator("#native-submit").click();
  await page.locator("#dag-select").selectOption("unicode-output-check");
  await page.locator("#native-start").click();
  await expect
    .poll(
      async () => {
        await page.locator("#refresh").click();
        return page.locator("#jobs").innerText();
      },
      { timeout: 10_000 },
    )
    .toContain("dead lettered");
  await expect(page.locator("#explanations")).toContainText("expand");
  await page.locator("#native-pause").click();
  await page.locator("#scenario").selectOption("delayed");
  await page.locator("#native-prepare").click();
  await page.locator("#native-submit").click();
  await page.locator("#dag-select").selectOption("timed-verification");
  await page.locator("#native-start").click();
  await expect
    .poll(
      async () =>
        page.evaluate(
          async () =>
            (await window.orchestratorNative.request("/native/status"))
              .active_jobs,
        ),
      { timeout: 5000 },
    )
    .toBe(1);
  const exit = once(application.process(), "exit");
  await page.locator("#native-quit").click();
  expect(application.process().exitCode).toBeNull();
  await page.locator("#native-quit").click();
  await application.evaluate(({ BrowserWindow }) =>
    BrowserWindow.getAllWindows()[0].close(),
  );
  await expect
    .poll(
      () =>
        application.evaluate(({ BrowserWindow }) =>
          BrowserWindow.getAllWindows()[0].getTitle(),
        ),
      { timeout: 1500 },
    )
    .toContain("finishing database and worker work");
  let quitTimer;
  const exitResult = await Promise.race([
    exit,
    new Promise((_, reject) => {
      quitTimer = setTimeout(
        () =>
          reject(new Error("Actual pending-work app exit did not complete")),
        45_000,
      );
    }),
  ]).finally(() => clearTimeout(quitTimer));
  expect(exitResult).toEqual([0, null]);
  current = undefined;
  expect(
    await stat(join(scratch, "profile/postgres/postmaster.pid")).then(
      () => true,
      () => false,
    ),
  ).toBe(false);
  const second = await launch();
  await second.page.locator("#dag-select").selectOption("release-verification");
  await showCompleted(second.page);
  await second.page.locator("#dag-select").selectOption("timed-verification");
  await expect(second.page.locator("#jobs")).toContainText("slept:2500ms");
  // Shutdown stops admission after the in-flight job; an unclaimed dependent remains genuinely ready.
  await expect(second.page.locator("#jobs")).toContainText("ready");
  await second.page.screenshot({
    path: join(output, "native-retained-work.png"),
    fullPage: true,
  });
  await finish(second.application, second.page);
  expect(externalRequests).toEqual([]);
  expect(proxyRequests).toBe(0);
  expect(pageErrors).toEqual([]);
  await writeFile(
    join(output, "acceptance.json"),
    JSON.stringify(
      {
        schema: 1,
        passed: true,
        version: metadata.version,
        source: metadata.source,
        sourceDirty: metadata.sourceDirty,
        actualLaunches: 2,
        normalExitCodes: [0, 0],
        visibleNativeWindow: true,
        actualPostgres: "18.6",
        realWorkerOutputs: true,
        retainedWorkCompleted: true,
        capturedResultsSha256: createHash("sha256")
          .update(await readFile(saved))
          .digest("hex"),
        externalRequests: 0,
        inheritedProxyRequests: 0,
        normalUserProfileOpened: false,
      },
      null,
      2,
    ) + "\n",
    { flag: "wx" },
  );
  console.log(
    JSON.stringify({ passed: true, actualLaunches: 2, postgres: "18.6" }),
  );
} catch (error) {
  await writeFile(
    join(output, "failure.json"),
    JSON.stringify({ message: String(error), externalRequests, pageErrors }) +
      "\n",
    { flag: "wx" },
  );
  if (current) {
    await current.evaluate(({ app }) => app.quit()).catch(() => {});
  }
  throw error;
} finally {
  await new Promise((resolve, reject) =>
    inertProxy.close((error) => (error ? reject(error) : resolve())),
  );
}
