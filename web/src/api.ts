/** File: Provides typed fetch adapters for paginated DAG lists and snapshots. Functions: fetchDagIds and fetchSnapshot enforce HTTP success. Variables: baseUrl remains caller-controlled for tests and deployments. */
import type { DagSnapshot } from "./model.js";

interface DagListResponse {
  readonly items: readonly string[];
  readonly next_cursor: string | null;
}

async function checkedJson<T>(response: Response): Promise<T> {
  if (!response.ok) throw new Error(`request failed with status ${response.status}`);
  return await response.json() as T;
}

export async function fetchDagIds(baseUrl = ""): Promise<readonly string[]> {
  return (await checkedJson<DagListResponse>(await fetch(`${baseUrl}/v1/dags?limit=100`))).items;
}

export async function fetchSnapshot(dagId: string, baseUrl = ""): Promise<DagSnapshot> {
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(dagId)) throw new Error("invalid DAG identifier");
  return await checkedJson<DagSnapshot>(await fetch(`${baseUrl}/v1/dags/${encodeURIComponent(dagId)}`));
}

