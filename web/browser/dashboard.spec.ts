/** File: Jason Stys: real dashboard use against explicit API contract fixtures, separate from actual PostgreSQL/API acceptance. */
import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { readFile } from "node:fs/promises";

function inspection(id: string) {
  return {
    snapshot: {
      id,
      jobs: [
        {
          id: "ship",
          state: "pending",
          attempts: 0,
          max_attempts: 3,
          lease_owner: "<img src=x onerror=alert(1)>",
        },
      ],
    },
    observed_at_ms: 1000,
    explanations: [
      {
        id: "ship",
        unmet_parents: ["verify"],
        terminal_blockers: ["prepare"],
        terminal_blocker_count: 1,
        terminal_blockers_truncated: false,
        retry_wait_remaining_ms: 25,
        lease_remaining_ms: 0,
        lease_expired_at_snapshot: true,
      },
    ],
    events: [
      {
        sequence: "12",
        job_id: "prepare",
        event_type: "dead_lettered",
        attempt: 3,
        occurred_at_ms: 900,
      },
    ],
    events_truncated: true,
    notice: "Captured fixture; no live countdown or failure-cause inference.",
  };
}

test.beforeEach(async ({ page }) => {
  await page.route("**/v1/dags?*", async (route) => {
    const after = new URL(route.request().url()).searchParams.get("after");
    await route.fulfill({
      json: after
        ? { items: ["next_flow"], next_cursor: null }
        : { items: ["alpha", "beta"], next_cursor: "beta" },
    });
  });
  await page.route("**/v1/dags/*/inspection", async (route) =>
    route.fulfill({
      json: inspection(route.request().url().split("/").at(-2)!),
    }),
  );
});

test("operator selects, preserves selection, pages, reads evidence and downloads exact captured JSON", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("#status")).toHaveText("Showing alpha");
  await page.getByLabel("Workflow", { exact: true }).selectOption("beta");
  await expect(page.locator("#status")).toHaveText("Showing beta");
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await expect(page.locator("#status")).toHaveText("Showing beta");
  await expect(
    page.getByRole("region", { name: "Dependency and lease explanations" }),
  ).toContainText("Expired; not reaped");
  await expect(page.locator("#event-window")).toContainText(
    "older history is omitted",
  );
  await expect(page.locator("#jobs")).toContainText(
    "<img src=x onerror=alert(1)>",
  );
  await expect(page.locator("#jobs img")).toHaveCount(0);
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page
      .getByRole("button", { name: "Download captured inspection JSON" })
      .click(),
  ]);
  const saved = JSON.parse(await readFile((await download.path())!, "utf8"));
  expect(saved).toEqual(inspection("beta"));
  await page
    .getByRole("button", { name: "Next workflows", exact: true })
    .click();
  await expect(page.locator("#status")).toHaveText("Showing next_flow");
  await expect(
    page.getByRole("button", { name: "Next workflows", exact: true }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "Previous workflows", exact: true })
    .click();
  await expect(page.locator("#status")).toHaveText("Showing alpha");
  expect(
    (
      await new AxeBuilder({ page })
        .withTags(["wcag2a", "wcag2aa", "wcag21aa"])
        .analyze()
    ).violations,
  ).toEqual([]);
});

test("refresh exposes an incomplete state until its real captured response is rendered", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("#status")).toHaveText("Showing alpha");
  let release: (() => void) | undefined;
  await page.route("**/v1/dags?*", async (route) => {
    await new Promise<void>((resolve) => {
      release = resolve;
    });
    await route.fulfill({ json: { items: ["alpha"], next_cursor: null } });
  });
  const refresh = page.locator("#refresh");
  await refresh.click();
  await expect.poll(() => Boolean(release)).toBe(true);
  await expect(refresh).toBeDisabled();
  await expect(page.locator("#jobs tr")).toHaveCount(0);
  // An immediate read after click is deliberately incomplete, not a completed-worker observation.
  expect(await page.locator("#jobs").innerText()).toBe("");
  release!();
  await expect(refresh).toBeEnabled();
  await expect(page.locator("#status")).toHaveText("Showing alpha");
  await expect(page.locator("#jobs")).toContainText("ship");
});

test("planner handles invalid JSON, rejects HTTP failure and invalidates an edited in-flight draft", async ({
  page,
}) => {
  let release: (() => void) | undefined;
  await page.route("**/v1/plans", async (route) => {
    await new Promise<void>((resolve) => {
      release = resolve;
    });
    await route.fulfill({
      json: {
        dag_id: "example",
        jobs: [{ id: "prepare", parents: [], dependency_wave: 0 }],
        notice: "No submission.",
      },
    });
  });
  await page.goto("/");
  const input = page.getByLabel("Workflow JSON", { exact: true });
  const button = page.getByRole("button", {
    name: "Preview dependency plan",
    exact: true,
  });
  await input.fill("{");
  await button.click();
  await expect(page.locator("#plan-status")).not.toContainText("Validating");
  await input.fill('{"id":"example"}');
  await button.click();
  await expect.poll(() => Boolean(release)).toBe(true);
  await input.fill('{"id":"new-draft"}');
  release!();
  await expect(page.locator("#plan-status")).toHaveText(
    "Draft changed; preview again.",
  );
  await expect(page.locator("#plan-rows li")).toHaveCount(0);
  await page.unroute("**/v1/plans");
  await page.route("**/v1/plans", (route) =>
    route.fulfill({ status: 422, body: "Invalid dependency" }),
  );
  await button.click();
  await expect(page.locator("#plan-status")).toContainText("422");
  await page.unroute("**/v1/plans");
  await page.route("**/v1/plans", (route) =>
    route.fulfill({
      json: {
        dag_id: "new-draft",
        jobs: [{ id: "prepare", parents: [], dependency_wave: 0 }],
        notice: "No submission.",
      },
    }),
  );
  await button.focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#plan-rows")).toContainText(
    "prepare — dependency wave 0; parents: none",
  );
});

test("empty/error state removes stale exports and newer selection wins a late response", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator("#status")).toHaveText("Showing alpha");
  let release: (() => void) | undefined;
  await page.route("**/v1/dags/beta/inspection", async (route) => {
    await new Promise<void>((resolve) => {
      release = resolve;
    });
    await route.fulfill({ json: inspection("beta") });
  });
  await page.getByLabel("Workflow", { exact: true }).selectOption("beta");
  await expect.poll(() => Boolean(release)).toBe(true);
  await page.getByLabel("Workflow", { exact: true }).selectOption("alpha");
  await expect(page.locator("#status")).toHaveText("Showing alpha");
  const delayedResponse = page.waitForResponse((response) =>
    response.url().endsWith("/v1/dags/beta/inspection"),
  );
  release!();
  await (await delayedResponse).finished();
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => requestAnimationFrame(() => resolve())),
  );
  await expect(page.locator("#status")).toHaveText("Showing alpha");
  await page.route("**/v1/dags?*", (route) =>
    route.fulfill({ json: { items: [], next_cursor: null } }),
  );
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await expect(page.locator("#status")).toHaveText("No workflows submitted");
  await expect(
    page.getByRole("button", { name: "Download captured inspection JSON" }),
  ).toBeDisabled();
  await expect(page.locator("#jobs tr")).toHaveCount(0);
  await page.route("**/v1/dags?*", (route) => route.fulfill({ status: 503 }));
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await expect(page.locator("#status")).toContainText("503");
  await expect(
    page.getByRole("button", { name: "Download captured inspection JSON" }),
  ).toBeDisabled();
});
