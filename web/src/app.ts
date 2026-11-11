/** File: Jason Stys: read-only DAG operator and pre-submission planning journeys.
 * Functions: refresh/load/render maintain captured state; handlers guard stale responses.
 * Variables: generation identifies latest navigation; cursors are tab-local; nothing is persisted. */
import {
  fetchDagPage,
  fetchInspection,
  fetchSnapshot,
  nativeBridge,
  previewPlan,
} from "./api.js";
import {
  stateClass,
  summarizeStates,
  type DagInspection,
  type DagSnapshot,
} from "./model.js";

function element<T extends Element>(selector: string): T {
  const value = document.querySelector<T>(selector);
  if (!value) throw new Error(`dashboard element is missing: ${selector}`);
  return value;
}

const select = element<HTMLSelectElement>("#dag-select");
const refreshButton = element<HTMLButtonElement>("#refresh");
const previous = element<HTMLButtonElement>("#previous-page");
const next = element<HTMLButtonElement>("#next-page");
const download = element<HTMLButtonElement>("#download-inspection");
const status = element<HTMLElement>("#status");
const cursors: (string | null)[] = [null];
let nextCursor: string | null = null;
let generation = 0;
let captured: DagInspection | null = null;
let capturedResults: DagSnapshot | null = null;

function clearInspection(): void {
  captured = null;
  capturedResults = null;
  element<HTMLButtonElement>("#native-results").disabled = true;
  download.disabled = true;
  for (const id of ["summary", "jobs", "explanations", "events"])
    element(`#${id}`).replaceChildren();
  element("#event-window").textContent = "";
  element("#inspection-notice").textContent =
    "No captured inspection is available.";
}

function renderSnapshot(snapshot: DagSnapshot): void {
  element("#summary").replaceChildren(
    ...summarizeStates(snapshot.jobs).map(({ state, count }) => {
      const card = document.createElement("article");
      card.className = `metric ${stateClass(state)}`;
      const value = document.createElement("strong");
      value.textContent = String(count);
      const label = document.createElement("span");
      label.textContent = state.replaceAll("_", " ");
      card.append(value, label);
      return card;
    }),
  );
  element("#jobs").replaceChildren(
    ...snapshot.jobs.map((job) =>
      tableRow([
        job.id,
        job.state.replaceAll("_", " "),
        `${job.attempts}/${job.max_attempts}`,
        job.lease_owner ?? "—",
        job.result ?? "—",
      ]),
    ),
  );
}

/** Insert supplied values literally; neither report text nor event metadata becomes HTML. */
function tableRow(values: readonly string[]): HTMLTableRowElement {
  const row = document.createElement("tr");
  for (const value of values) {
    const cell = document.createElement("td");
    cell.textContent = value;
    row.append(cell);
  }
  return row;
}

function renderInspection(value: DagInspection): void {
  captured = value;
  download.disabled = false;
  renderSnapshot(value.snapshot);
  element("#inspection-notice").textContent =
    `Captured ${new Date(value.observed_at_ms).toISOString()}. ${value.notice}`;
  element("#explanations").replaceChildren(
    ...value.explanations.map((row) =>
      tableRow([
        row.id,
        row.unmet_parents.join(", ") || "None",
        row.terminal_blockers_truncated
          ? `${row.terminal_blockers.join(", ") || "No IDs listed"} (${row.terminal_blocker_count} total; bounded list truncated)`
          : row.terminal_blockers.join(", ") || "None",
        row.retry_wait_remaining_ms === null
          ? "—"
          : `${row.retry_wait_remaining_ms} ms`,
        row.lease_remaining_ms === null
          ? "—"
          : row.lease_expired_at_snapshot
            ? "Expired; not reaped"
            : `${row.lease_remaining_ms} ms`,
      ]),
    ),
  );
  element("#event-window").textContent = value.events_truncated
    ? "Newest 200 recorded events only; older history is omitted."
    : value.events.length
      ? "All currently recorded events, newest first."
      : "No recorded attempt events. This does not prove no work occurred.";
  element("#events").replaceChildren(
    ...value.events.map((event) => {
      const item = document.createElement("li");
      item.textContent = `#${event.sequence} ${event.job_id}: ${event.event_type}, attempt ${event.attempt}, ${new Date(event.occurred_at_ms).toISOString()}`;
      return item;
    }),
  );
}

/** Load one workflow; late responses cannot replace newer selections or enable stale exports. */
async function load(id: string, request: number): Promise<void> {
  const [value, results] = await Promise.all([
    fetchInspection(id),
    nativeBridge() ? fetchSnapshot(id) : Promise.resolve(null),
  ]);
  if (generation !== request) return;
  renderInspection(value);
  if (results) {
    capturedResults = results;
    renderSnapshot(results);
    element<HTMLButtonElement>("#native-results").disabled = false;
  }
  status.textContent = `Showing ${id}`;
}

function finish(request: number): void {
  if (request !== generation) return;
  refreshButton.disabled = false;
  select.disabled = false;
  previous.disabled = cursors.length === 1;
  next.disabled = nextCursor === null;
}

async function refresh(): Promise<void> {
  const request = ++generation;
  const wanted = select.value;
  refreshButton.disabled = true;
  select.disabled = true;
  previous.disabled = true;
  next.disabled = true;
  clearInspection();
  status.textContent = "Refreshing captured state…";
  nextCursor = null;
  select.replaceChildren();
  try {
    const page = await fetchDagPage(cursors.at(-1) ?? null);
    if (generation !== request) return;
    nextCursor = page.next_cursor;
    select.replaceChildren(...page.items.map((id) => new Option(id, id)));
    if (page.items.includes(wanted)) select.value = wanted;
    if (select.value) await load(select.value, request);
    else
      status.textContent =
        cursors.length === 1
          ? "No workflows submitted"
          : "No more workflows on this page; use Previous workflows.";
  } catch (error) {
    if (generation === request) {
      clearInspection();
      status.textContent =
        error instanceof Error ? error.message : "Unable to load state";
    }
  } finally {
    finish(request);
  }
}

select.addEventListener("change", () => {
  const request = ++generation;
  clearInspection();
  refreshButton.disabled = true;
  status.textContent = "Loading selected workflow…";
  void load(select.value, request)
    .catch((error: unknown) => {
      if (generation === request)
        status.textContent =
          error instanceof Error ? error.message : "Unable to load workflow";
    })
    .finally(() => finish(request));
});
refreshButton.addEventListener("click", () => void refresh());
previous.addEventListener("click", () => {
  if (cursors.length > 1) {
    cursors.pop();
    void refresh();
  }
});
next.addEventListener("click", () => {
  if (nextCursor !== null) {
    cursors.push(nextCursor);
    void refresh();
  }
});
download.addEventListener("click", () => {
  if (captured === null) return;
  const bridge = nativeBridge();
  if (bridge) {
    void bridge
      .exportCaptured(captured.snapshot.id, captured)
      .catch((error) => {
        status.textContent = String(error);
      });
    return;
  }
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(captured, null, 2)], { type: "application/json" }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = `${captured.snapshot.id}-inspection.json`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
});

const input = element<HTMLTextAreaElement>("#plan-input");
const planStatus = element<HTMLElement>("#plan-status");
const planRows = element<HTMLOListElement>("#plan-rows");
let planGeneration = 0;
input.value = JSON.stringify(
  {
    id: "example",
    jobs: [
      { id: "prepare", kind: "uppercase", payload: "synthetic" },
      { id: "verify", kind: "checksum", payload: "synthetic" },
    ],
    dependencies: [{ from: "prepare", to: "verify" }],
  },
  null,
  2,
);
input.addEventListener("input", () => {
  ++planGeneration;
  planRows.replaceChildren();
  planStatus.textContent = "Draft changed; preview again.";
});
element<HTMLFormElement>("#plan-form").addEventListener("submit", (event) => {
  event.preventDefault();
  const request = ++planGeneration;
  const text = input.value;
  planRows.replaceChildren();
  planStatus.textContent = "Validating without submission…";
  void previewPlan(text)
    .then((result) => {
      if (request !== planGeneration || input.value !== text) return;
      planStatus.textContent = `${result.dag_id}: ${result.notice}`;
      planRows.replaceChildren(
        ...result.jobs.map((row) => {
          const item = document.createElement("li");
          item.textContent = `${row.id} — dependency wave ${row.dependency_wave}; parents: ${row.parents.join(", ") || "none"}`;
          return item;
        }),
      );
    })
    .catch((error: unknown) => {
      if (request === planGeneration)
        planStatus.textContent =
          error instanceof Error ? error.message : "Invalid workflow";
    });
});
/** Native controls operate the original durable API; prepared cases are inputs, never a fake backend. */
const bridge = nativeBridge();
if (bridge) {
  element<HTMLElement>("#native-tools").hidden = false;
  element<HTMLElement>("#native-authoring").hidden = false;
  const notice = element<HTMLElement>("#native-status");
  async function inspectOwner(): Promise<void> {
    const value = (await bridge!.request("/native/status")) as {
      version: string;
      pause_requested: boolean;
      active_jobs: number;
      workers: number;
    };
    notice.textContent = `Version ${value.version} · ${value.pause_requested ? "Pause requested" : "Workers enabled"} · ${value.active_jobs} active · ${value.workers} workers. Refresh for a new captured observation.`;
  }
  async function operation(action: () => Promise<unknown>): Promise<void> {
    const buttons = [
      ...document.querySelectorAll<HTMLButtonElement>(
        "#native-tools button, #native-authoring button",
      ),
    ];
    buttons.forEach((button) => {
      button.disabled = true;
    });
    try {
      await action();
      await inspectOwner();
      await refresh();
    } catch (error) {
      notice.textContent =
        error instanceof Error
          ? error.message
          : "Operation failed; refresh retained state.";
    } finally {
      buttons.forEach((button) => {
        button.disabled = false;
      });
      element<HTMLButtonElement>("#native-results").disabled =
        capturedResults === null;
    }
  }
  for (const [selector, paused] of [
    ["#native-start", false],
    ["#native-pause", true],
  ] as const)
    element(selector).addEventListener(
      "click",
      () =>
        void operation(() =>
          bridge.request("/native/workers", "POST", { paused }),
        ),
    );
  element("#native-reap").addEventListener(
    "click",
    () => void operation(() => bridge.request("/native/reap", "POST", {})),
  );
  element("#native-cancel").addEventListener(
    "click",
    () =>
      void operation(async () => {
        if (
          !select.value ||
          !confirm(
            `Cancel ${select.value}? Committed results/history remain; running leases are revoked.`,
          )
        )
          return;
        await bridge.request(`/v1/dags/${select.value}/cancel`, "POST", {});
      }),
  );
  element("#native-quit").addEventListener("click", () => void bridge.quit());
  element("#native-prepare").addEventListener("click", () => {
    const scenario = element<HTMLSelectElement>("#scenario").value;
    const data =
      scenario === "unicode"
        ? {
            id: "unicode-output-check",
            jobs: [
              {
                id: "expand",
                kind: "uppercase",
                payload: "ΐ".repeat(2048),
                max_attempts: 1,
              },
              { id: "downstream", kind: "checksum", payload: "abc" },
            ],
            dependencies: [{ from: "expand", to: "downstream" }],
          }
        : scenario === "telemetry"
          ? {
              id: "telemetry-processing",
              jobs: [
                {
                  id: "normalize",
                  kind: "uppercase",
                  payload: "sensor a:ready",
                },
                {
                  id: "fingerprint",
                  kind: "checksum",
                  payload: "sensor a:ready",
                },
              ],
              dependencies: [{ from: "normalize", to: "fingerprint" }],
            }
          : scenario === "delayed"
            ? {
                id: "timed-verification",
                jobs: [
                  { id: "wait", kind: "delay", payload: "2500" },
                  { id: "check", kind: "checksum", payload: "abc" },
                ],
                dependencies: [{ from: "wait", to: "check" }],
              }
            : {
                id: "release-verification",
                jobs: [
                  { id: "label", kind: "uppercase", payload: "release ready" },
                  { id: "verify", kind: "checksum", payload: "abc" },
                ],
                dependencies: [{ from: "label", to: "verify" }],
              };
    input.value = JSON.stringify(data, null, 2);
    input.dispatchEvent(new Event("input"));
    notice.textContent =
      "Prepared editable input only; preview and explicitly submit to persist it.";
  });
  element("#native-import").addEventListener(
    "click",
    () =>
      void operation(async () => {
        const text = await bridge.importWorkflow();
        if (text !== null) {
          input.value = text;
          input.dispatchEvent(new Event("input"));
        }
      }),
  );
  element("#native-submit").addEventListener(
    "click",
    () =>
      void operation(async () => {
        if (new TextEncoder().encode(input.value).length > 1_000_000)
          throw new Error("Workflow exceeds 1 MB");
        await bridge.request("/v1/dags", "POST", input.value);
      }),
  );
  element("#native-results").addEventListener("click", () => {
    if (capturedResults)
      void bridge
        .exportCaptured(capturedResults.id, capturedResults)
        .catch((error) => {
          notice.textContent = String(error);
        });
  });
  refreshButton.addEventListener(
    "click",
    () =>
      void inspectOwner().catch((error) => {
        notice.textContent = String(error);
      }),
  );
  void inspectOwner().catch((error) => {
    notice.textContent = String(error);
  });
}
void refresh();
