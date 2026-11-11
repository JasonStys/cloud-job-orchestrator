/** File: Provides typed fetch adapters for paginated DAG lists and snapshots. Functions: fetchDagIds and fetchSnapshot enforce HTTP success. Variables: baseUrl remains caller-controlled for tests and deployments. */
import type { DagSnapshot, DagInspection, GraphPlan } from "./model.js";

export interface NativeBridge {
  request(route: string, method?: string, body?: unknown): Promise<unknown>;
  importWorkflow(): Promise<string | null>;
  exportCaptured(identifier: string, value: unknown): Promise<boolean>;
  quit(): Promise<boolean>;
}
declare global {
  interface Window {
    readonly orchestratorNative?: NativeBridge;
  }
}

/** The normal deployed API stays unchanged; only the bundled sandbox has this fixed IPC bridge. */
export function nativeBridge(): NativeBridge | undefined {
  return typeof window === "undefined" ? undefined : window.orchestratorNative;
}

async function send(route: string, init?: RequestInit): Promise<Response> {
  const bridge = nativeBridge();
  if (!bridge) return fetch(route, init);
  const value = await bridge.request(
    route,
    init?.method ?? "GET",
    typeof init?.body === "string" ? init.body : undefined,
  );
  return new Response(JSON.stringify(value), {
    headers: { "Content-Type": "application/json" },
  });
}

export interface DagListResponse {
  readonly items: readonly string[];
  readonly next_cursor: string | null;
}

export async function fetchDagPage(
  after: string | null,
): Promise<DagListResponse> {
  if (after !== null && !/^[A-Za-z0-9_-]{1,64}$/.test(after))
    throw new Error("invalid page cursor");
  return await checkedJson<DagListResponse>(
    await send(
      `/v1/dags?limit=25${after === null ? "" : `&after=${encodeURIComponent(after)}`}`,
    ),
  );
}

export async function fetchInspection(id: string): Promise<DagInspection> {
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(id))
    throw new Error("invalid DAG identifier");
  return await checkedJson<DagInspection>(
    await send(`/v1/dags/${encodeURIComponent(id)}/inspection`),
  );
}

export async function previewPlan(text: string): Promise<GraphPlan> {
  if (new TextEncoder().encode(text).byteLength > 1_000_000)
    throw new Error("Plan exceeds 1 MB");
  const body: unknown = JSON.parse(text);
  return await checkedJson<GraphPlan>(
    await send("/v1/plans", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: nativeBridge() ? text : JSON.stringify(body),
    }),
  );
}

async function checkedJson<T>(response: Response): Promise<T> {
  if (!response.ok)
    throw new Error(`request failed with status ${response.status}`);
  return (await response.json()) as T;
}

export async function fetchDagIds(baseUrl = ""): Promise<readonly string[]> {
  return (
    await checkedJson<DagListResponse>(
      await fetch(`${baseUrl}/v1/dags?limit=100`),
    )
  ).items;
}

export async function fetchSnapshot(
  dagId: string,
  baseUrl = "",
): Promise<DagSnapshot> {
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(dagId))
    throw new Error("invalid DAG identifier");
  return await checkedJson<DagSnapshot>(
    await send(`${baseUrl}/v1/dags/${encodeURIComponent(dagId)}`),
  );
}
