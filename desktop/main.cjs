/** File: desktop/main.cjs
 * Purpose: Own the actual bundled database service and sandboxed full native dashboard window.
 * Functions: start verifies package/owner; RPC uses exact sender; Quit retains real pending service work.
 * State: caches use a fresh owned temporary root; durable data stays outside program resources.
 */
"use strict";
const { app, BrowserWindow, Menu, ipcMain, dialog } = require("electron");
const fs = require("node:fs");
const { join, resolve, dirname, sep } = require("node:path");
const { tmpdir } = require("node:os");
const { createHash } = require("node:crypto");
const { startNative, VERSION, MAX_BODY } = require("./runtime.cjs");
const { readRegular, readUtf8, newFile } = require("./files.cjs");

const shellRoot = fs.mkdtempSync(join(tmpdir(), "cjo-owned-shell-"));
app.setPath("userData", join(shellRoot, "cache"));
const acceptance = process.env.CJO_NATIVE_ACCEPTANCE_ROOT;
let profile;
if (acceptance) {
  if (
    readRegular(join(resolve(acceptance), "acceptance.marker"), 128).toString(
      "utf8",
    ) !== "cjo-owned-native-test\n"
  )
    throw new Error("Owned synthetic acceptance marker required.");
  profile = join(resolve(acceptance), "profile");
} else {
  const position = process.argv.indexOf("--profile");
  if (position >= 0) {
    if (
      !process.argv[position + 1] ||
      process.argv.indexOf("--profile", position + 1) >= 0
    )
      throw new Error("Exactly one profile path is required.");
    profile = resolve(process.argv[position + 1]);
  } else {
    if (!process.env.LOCALAPPDATA)
      throw new Error(
        "Local app data is unavailable; select an explicit profile.",
      );
    profile = join(
      process.env.LOCALAPPDATA,
      "JasonStysPortfolio",
      "cloud-job-orchestrator",
      "workspace",
    );
  }
}
let window,
  service,
  startup,
  closing,
  exiting = false;

/** Refuse foreign IPC senders and close admission before any retained background operation drains. */
function sender(event, allowClosing = false) {
  if (
    !window ||
    event.sender !== window.webContents ||
    event.senderFrame !== window.webContents.mainFrame ||
    !service ||
    event.senderFrame.url !== service.origin + "/"
  )
    throw new Error("Owned main window required.");
  if (closing && !allowClosing)
    throw new Error("The application is finishing saved work.");
}

function closeOwned() {
  if (!closing)
    closing = (async () => {
      if (window && !window.isDestroyed())
        window.setTitle(
          "Cloud Job Orchestrator — finishing database and worker work",
        );
      try {
        await startup;
        await service?.close();
      } catch {
        process.exitCode = 1;
        dialog.showErrorBox(
          "Native close needs review",
          "The owned service did not finish normally. Preserve the local workspace; no database reset or forced termination was attempted.",
        );
      }
      exiting = true;
      app.quit();
    })();
  return closing;
}
app.on("before-quit", (event) => {
  if (!exiting) {
    event.preventDefault();
    void closeOwned();
  }
});
app.on("window-all-closed", () => {
  if (!exiting) void closeOwned();
});

async function start() {
  const resources = app.isPackaged
    ? join(process.resourcesPath, "native")
    : resolve(__dirname, "../.runtime/native-resources");
  if (
    app.isPackaged &&
    [dirname(process.execPath), resources].some((directory) => {
      const candidate = resolve(profile).toLowerCase(),
        program = resolve(directory).toLowerCase();
      return candidate === program || candidate.startsWith(program + sep);
    })
  )
    throw new Error(
      "Choose a durable workspace outside the application installation and resources.",
    );
  const metadata = JSON.parse(
    readRegular(join(resources, "SOURCE.json"), 1_048_576).toString("utf8"),
  );
  if (
    metadata.product !== "cloud-job-orchestrator" ||
    metadata.version !== VERSION ||
    !/^[0-9a-f]{40}$/.test(metadata.source) ||
    typeof metadata.sourceDirty !== "boolean"
  )
    throw new Error("Native package identity is invalid.");
  const modules = ["files.cjs", "main.cjs", "preload.cjs", "runtime.cjs"];
  if (Object.keys(metadata.asar).length !== modules.length)
    throw new Error("Native owner inventory differs.");
  for (const name of modules) {
    const content = fs.readFileSync(join(__dirname, name));
    if (
      content.length > MAX_BODY ||
      createHash("sha256").update(content).digest("hex") !==
        metadata.asar["desktop/" + name]
    )
      throw new Error("Native owner bytes differ from their package.");
  }
  for (const name of [
    "app.js",
    "api.js",
    "model.js",
    "styles.css",
    "index.html",
  ]) {
    const path = "web/dist/" + name;
    if (
      createHash("sha256")
        .update(readRegular(join(resources, path), MAX_BODY))
        .digest("hex") !== metadata.files[path]
    )
      throw new Error("Native dashboard bytes differ from their package.");
  }
  const binary = join(resources, "bin/orchestrator-native.exe");
  const executable = readRegular(binary, 128 * 1024 * 1024);
  if (
    createHash("sha256").update(executable).digest("hex") !==
    metadata.files["bin/orchestrator-native.exe"]
  )
    throw new Error("Native binary differs from its package.");
  const startupPage =
    "data:text/html;charset=utf-8," +
    encodeURIComponent(
      "<!doctype html><html lang='en'><meta charset='utf-8'><meta http-equiv='Content-Security-Policy' content=\"default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\"><title>Cloud Job Orchestrator — starting local database</title><style>body{font:20px Segoe UI,sans-serif;background:#0c1c22;color:#e5f7f8;margin:64px;line-height:1.6}h1{font-size:34px}p{max-width:850px}</style><h1>Starting your local job workspace</h1><p id='native-startup-status' role='status'>Preparing the owned PostgreSQL database and verifying saved state. First launch takes a little longer. This window stays available while startup finishes.</p><p>Use Application → Quit after saved work to close safely. Help describes the full workflow. Saved jobs remain local; no cloud service or Docker is started.</p><p>Cloud Job Orchestrator 1.1.0 · Created by Jason Stys · github.com/JasonStys</p></html>",
    );
  window = new BrowserWindow({
    width: 1280,
    height: 900,
    minWidth: 600,
    minHeight: 500,
    title: "Cloud Job Orchestrator",
    webPreferences: {
      preload: join(__dirname, "preload.cjs"),
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      devTools: Boolean(acceptance),
    },
  });
  const session = window.webContents.session;
  await session.setProxy({ mode: "direct" });
  session.setPermissionRequestHandler((_contents, _permission, callback) =>
    callback(false),
  );
  session.setPermissionCheckHandler(() => false);
  session.webRequest.onBeforeRequest((details, callback) =>
    callback({
      cancel:
        details.url !== startupPage &&
        !(service && details.url.startsWith(service.origin + "/")),
    }),
  );
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  window.webContents.on("will-navigate", (event, url) => {
    if (url !== startupPage && (!service || url !== service.origin + "/"))
      event.preventDefault();
  });
  window.on("close", (event) => {
    if (!exiting) {
      event.preventDefault();
      void closeOwned();
    }
  });
  Menu.setApplicationMenu(
    Menu.buildFromTemplate([
      {
        label: "Application",
        submenu: [
          { label: "Quit after saved work", click: () => void closeOwned() },
        ],
      },
      {
        label: "Help",
        submenu: [
          {
            label: "Using this application",
            click: () =>
              dialog.showMessageBox(window, {
                type: "info",
                title: "Cloud Job Orchestrator",
                message:
                  "Prepare or import a workflow, preview its dependency plan, explicitly submit it, then start the two real workers. Refresh to inspect committed results and captured attempt events. Pause finishes in-flight jobs; Quit waits for real work and PostgreSQL shutdown. Reopen to retain jobs and history.",
                detail:
                  "By Jason Stys · github.com/JasonStys. Owner-local plaintext workspace; reference operations use a fixed safe allowlist, not arbitrary commands. Exports are captured snapshots, not full database backups. Unsigned Windows x64 portable app.",
              }),
          },
        ],
      },
    ]),
  );
  ipcMain.handle("cjo:request", async (event, value) => {
    sender(event);
    return service.request(value);
  });
  ipcMain.handle("cjo:import", async (event) => {
    sender(event);
    const chosen = await dialog.showOpenDialog(window, {
      properties: ["openFile"],
      filters: [{ name: "Workflow JSON", extensions: ["json"] }],
    });
    if (chosen.canceled) return null;
    let text;
    try {
      text = readUtf8(chosen.filePaths[0], MAX_BODY);
    } catch {
      throw new Error(
        "Choose a regular UTF-8 JSON document within the 1 MiB limit; prior editor data is retained.",
      );
    }
    JSON.parse(text);
    return text;
  });
  ipcMain.handle("cjo:export", async (event, value) => {
    sender(event);
    if (!value || !/^[A-Za-z0-9_-]{1,64}$/.test(value.identifier))
      throw new Error("Invalid captured identifier");
    const text = JSON.stringify(value.value, null, 2);
    if (Buffer.byteLength(text) > MAX_BODY)
      throw new Error("Captured evidence exceeds 1 MiB");
    const chosen = await dialog.showSaveDialog(window, {
      defaultPath: value.identifier + "-captured.json",
      filters: [{ name: "Captured JSON", extensions: ["json"] }],
    });
    if (chosen.canceled) return false;
    try {
      newFile(chosen.filePath, text + "\n");
    } catch (error) {
      if (error.code === "EEXIST")
        throw new Error(
          "Destination already exists; choose a new file. Nothing was overwritten.",
        );
      throw error;
    }
    return true;
  });
  ipcMain.handle("cjo:quit", (event) => {
    sender(event, true);
    void closeOwned();
    return true;
  });
  await window.loadURL(startupPage);
  service = await startNative({ binary, resources, profile, metadata });
  await window.loadURL(service.origin + "/");
  service.completion.then(
    () => {
      if (!closing) {
        process.exitCode = 1;
        void closeOwned();
      }
    },
    () => {
      if (!closing) {
        process.exitCode = 1;
        void closeOwned();
      }
    },
  );
}
app.whenReady().then(() => {
  startup = start().catch((error) => {
    process.exitCode = 1;
    dialog.showErrorBox("Native startup needs review", error.message);
    throw error;
  });
  startup.catch(() => {
    void closeOwned();
  });
});
