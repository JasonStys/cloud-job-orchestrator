/** File: Renders the dependency-free operations dashboard with safe DOM APIs. Functions: refresh, renderDagList, and renderSnapshot update live status. Variables: cached element references bind the page shell to API data. */
import { fetchDagIds, fetchSnapshot } from "./api.js";
import { stateClass, summarizeStates, type DagSnapshot } from "./model.js";

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`dashboard element is missing: ${selector}`);
  return element;
}

const select = requiredElement<HTMLSelectElement>("#dag-select");
const refreshButton = requiredElement<HTMLButtonElement>("#refresh");
const summary = requiredElement<HTMLElement>("#summary");
const jobs = requiredElement<HTMLTableSectionElement>("#jobs");
const status = requiredElement<HTMLElement>("#status");

function renderDagList(ids: readonly string[]): void {
  select.replaceChildren(...ids.map((id) => new Option(id, id)));
}

function renderSnapshot(snapshot: DagSnapshot): void {
  summary.replaceChildren(...summarizeStates(snapshot.jobs).map(({ state, count }) => {
    const card = document.createElement("article");
    card.className = `metric ${stateClass(state)}`;
    const value = document.createElement("strong");
    value.textContent = String(count);
    const label = document.createElement("span");
    label.textContent = state.replace("_", " ");
    card.append(value, label);
    return card;
  }));
  jobs.replaceChildren(...snapshot.jobs.map((job) => {
    const row = document.createElement("tr");
    for (const value of [job.id, job.state.replace("_", " "), `${job.attempts}/${job.max_attempts}`, job.lease_owner ?? "—"]) {
      const cell = document.createElement("td");
      cell.textContent = value;
      row.append(cell);
    }
    return row;
  }));
}

async function refresh(): Promise<void> {
  refreshButton.disabled = true;
  status.textContent = "Refreshing control-plane state…";
  try {
    const ids = await fetchDagIds();
    renderDagList(ids);
    if (ids[0]) renderSnapshot(await fetchSnapshot(select.value || ids[0]));
    status.textContent = ids.length ? `Showing ${select.value}` : "No DAGs submitted";
  } catch (error: unknown) {
    status.textContent = error instanceof Error ? error.message : "Unable to load state";
  } finally {
    refreshButton.disabled = false;
  }
}

refreshButton.addEventListener("click", () => void refresh());
select.addEventListener("change", () => void refresh());
void refresh();
