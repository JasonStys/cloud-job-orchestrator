/** File: Defines dashboard API models and presentation helpers. Functions: summarizeStates and stateClass produce stable view data. Variables: records are immutable inputs from the control-plane API. */

export type JobState =
  | "pending"
  | "ready"
  | "retry_wait"
  | "leased"
  | "succeeded"
  | "dead_lettered"
  | "cancelled";

export interface JobSnapshot {
  readonly id: string;
  readonly state: JobState;
  readonly attempts: number;
  readonly max_attempts: number;
  readonly lease_owner: string | null;
}

export interface DagSnapshot {
  readonly id: string;
  readonly jobs: readonly JobSnapshot[];
}

export interface DagInspection {
  readonly snapshot: DagSnapshot;
  readonly observed_at_ms: number;
  readonly explanations: readonly {
    id: string;
    unmet_parents: readonly string[];
    terminal_blockers: readonly string[];
    terminal_blocker_count: number;
    terminal_blockers_truncated: boolean;
    retry_wait_remaining_ms: number | null;
    lease_remaining_ms: number | null;
    lease_expired_at_snapshot: boolean;
  }[];
  readonly events: readonly {
    sequence: string;
    job_id: string;
    event_type: string;
    attempt: number;
    occurred_at_ms: number;
  }[];
  readonly events_truncated: boolean;
  readonly notice: string;
}

export interface GraphPlan {
  readonly dag_id: string;
  readonly jobs: readonly {
    id: string;
    parents: readonly string[];
    dependency_wave: number;
  }[];
  readonly notice: string;
}

export interface StateCount {
  readonly state: JobState;
  readonly count: number;
}

/** Counts jobs by state in O(n) and emits a stable label order. */
export function summarizeStates(
  jobs: readonly JobSnapshot[],
): readonly StateCount[] {
  const order: readonly JobState[] = [
    "pending",
    "ready",
    "retry_wait",
    "leased",
    "succeeded",
    "dead_lettered",
    "cancelled",
  ];
  const counts = new Map<JobState, number>();
  for (const job of jobs)
    counts.set(job.state, (counts.get(job.state) ?? 0) + 1);
  return order
    .filter((state) => counts.has(state))
    .map((state) => ({ state, count: counts.get(state) ?? 0 }));
}

/** Returns a constrained CSS modifier for a server-validated state. */
export function stateClass(state: JobState): string {
  return `state-${state.replace("_", "-")}`;
}
