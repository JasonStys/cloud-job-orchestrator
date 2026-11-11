/** File: desktop/preload.cjs
 * Purpose: Expose only bounded owner-app operations to the sandboxed dashboard, never credentials.
 * Functions: frozen bridge methods forward literal operation names; state contains no private token.
 */
"use strict";
const { contextBridge, ipcRenderer } = require("electron");
contextBridge.exposeInMainWorld(
  "orchestratorNative",
  Object.freeze({
    request: (route, method = "GET", body = undefined) =>
      ipcRenderer.invoke("cjo:request", { route, method, body }),
    importWorkflow: () => ipcRenderer.invoke("cjo:import"),
    exportCaptured: (identifier, value) =>
      ipcRenderer.invoke("cjo:export", { identifier, value }),
    quit: () => ipcRenderer.invoke("cjo:quit"),
  }),
);
