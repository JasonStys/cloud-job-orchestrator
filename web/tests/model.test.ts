/** File: Tests dashboard aggregation and CSS-state mapping. Functions: node:test cases cover stable ordering and empty inputs. Variables: fixtures are immutable snapshots. */
import assert from "node:assert/strict";
import test from "node:test";
import { stateClass, summarizeStates, type JobSnapshot } from "../src/model.ts";

test("summarizeStates counts in stable operational order", () => {
  const jobs: readonly JobSnapshot[] = [
    { id: "b", state: "succeeded", attempts: 1, max_attempts: 3, lease_owner: null },
    { id: "a", state: "ready", attempts: 0, max_attempts: 3, lease_owner: null },
    { id: "c", state: "succeeded", attempts: 1, max_attempts: 3, lease_owner: null }
  ];
  assert.deepEqual(summarizeStates(jobs), [
    { state: "ready", count: 1 },
    { state: "succeeded", count: 2 }
  ]);
});

test("stateClass converts the only compound state", () => {
  assert.equal(stateClass("dead_lettered"), "state-dead-lettered");
  assert.deepEqual(summarizeStates([]), []);
});

