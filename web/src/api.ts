/** File: Provides typed fetch adapters for paginated DAG lists and snapshots. Functions: fetchDagIds and fetchSnapshot enforce HTTP success. Variables: baseUrl remains caller-controlled for tests and deployments. */
import type { DagSnapshot, DagInspection, GraphPlan } from "./model.js";

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
    await fetch(
      `/v1/dags?limit=25${after === null ? "" : `&after=${encodeURIComponent(after)}`}`,
    ),
  );
}

export async function fetchInspection(id: string): Promise<DagInspection> {
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(id))
    throw new Error("invalid DAG identifier");
  return await checkedJson<DagInspection>(
    await fetch(`/v1/dags/${encodeURIComponent(id)}/inspection`),
  );
}

export async function previewPlan(text: string): Promise<GraphPlan> {
  if (new TextEncoder().encode(text).byteLength > 1_000_000)
    throw new Error("Plan exceeds 1 MB");
  const body: unknown = JSON.parse(text);
  return await checkedJson<GraphPlan>(
    await fetch("/v1/plans", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
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
    await fetch(`${baseUrl}/v1/dags/${encodeURIComponent(dagId)}`),
  );
}
