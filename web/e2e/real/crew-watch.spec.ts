import { expect, test } from "@playwright/test";
import type { BackgroundCrew } from "../../src/generated/BackgroundCrew";
import type { BackgroundWatchChanges } from "../../src/generated/BackgroundWatchChanges";
import type { BackgroundWatchStatus } from "../../src/generated/BackgroundWatchStatus";
import type { EngineStatus } from "../../src/generated/EngineStatus";
import { startProject, type Service } from "./service";

/**
 * KC6 (M26), the watch runner against the real service: a background crew whose trigger is a watch runs a fixed
 * command on its schedule with no model call. This spec starts the service's own listener tick, creates a watch whose
 * command fails, and waits for its first check: the check is a baseline (recorded as Failing, never raising), the two
 * watch reads answer it, and the watch's page draws it. No engine is installed, so the check running at all proves it
 * made no model call.
 *
 * What this cannot reach, and why (the parity row says so): a change row (and with it the refusals and the template
 * start) needs a pass→fail turn between two `every_hours` windows, which the listener buckets by the hour
 * (`watch:<crew>:<window start>`), so a second check is up to an hour away; and a template start on this Linux box
 * would be refused by the host confinement (Crew workers are macOS-only in the beta) before any model.
 */

const WINDOW = { width: 1280, height: 792 };
const PAGE_READY = 30_000;
const CHECK_MS = 120_000;

let service: Service;
let projectId: string;

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ crewPreview: true, backgroundWatchIntervalSecs: 60 }));
});

test.afterAll(async () => {
  await service?.stop();
});

test("a watch's failing baseline runs its fixed command with no model call, and the page reads it", async ({ page }) => {
  test.setTimeout(240_000);
  await page.setViewportSize(WINDOW);

  // The default service start has no engine installed: the watch's command still runs.
  const engines = await service.api<EngineStatus[]>("GET", "/api/engines");
  expect(engines.every((engine) => engine.installed === false)).toBe(true);

  const crew = await service.api<BackgroundCrew>("POST", "/api/crew/background/create", {
    project_id: projectId,
    name: "Main build",
    template: "fix_failing_ci",
    trigger: { kind: "watch", command: "sh -c 'echo boom; exit 1'", every_hours: 1, on_failing: "needs_you" },
    budget: { turns: 10, minutes: 30 },
    daily: { runs: 1, turns: 10 },
    utc_offset_minutes: 0,
  });
  expect((await service.api<BackgroundCrew>("POST", `/api/crew/background/${crew.id}/enable`, {})).enabled).toBe(true);

  // The listener's next pass is one interval away; its first check of this watch is a baseline.
  await expect
    .poll(async () => (await service.api<BackgroundWatchStatus>("GET", `/api/crew/background/${crew.id}/watch`)).baseline_done, { timeout: CHECK_MS, intervals: [500, 1000, 2000] })
    .toBe(true);
  const watch = await service.api<BackgroundWatchStatus>("GET", `/api/crew/background/${crew.id}/watch`);
  expect(watch.state).toBe("failing");
  expect(watch.last_exit_code).toBe(1);
  expect(watch.tail).toContain("boom");
  expect(watch.reason ?? null).toBeNull();
  expect(watch.error_streak).toBe(0);
  const changes = await service.api<BackgroundWatchChanges>("GET", `/api/crew/background/${crew.id}/watch/changes?offset=0&limit=20`);
  expect(changes.total).toBe(1);
  expect(changes.changes[0]).toMatchObject({ kind: "baseline", action: "none", exit_code: 1 });
  // A failing baseline raises nothing: the one needs-you field has no item for it.
  const needs = await service.api<{ count: number }>("GET", "/api/crew/needs-you");
  expect(needs.count).toBe(0);
  // Run now does not start a watch: it has no listener to fire its command.
  const refused = await service.api("POST", `/api/crew/background/${crew.id}/run-now`, {}).then(
    () => {
      throw new Error("Run now started a run for a watch");
    },
    (error: Error) => error.message,
  );
  expect(refused).toContain("403");
  expect(refused).toContain("A watch crew's command runs on its schedule; Run now does not start a run for it.");

  // The web draws the list row and the watch's own page from the two real reads.
  const reads: { path: string; status: number }[] = [];
  page.on("response", (response) => {
    const path = new URL(response.url()).pathname;
    if (response.request().method() === "GET" && path.startsWith(`/api/crew/background/${crew.id}/watch`)) reads.push({ path, status: response.status() });
  });
  await page.goto(`${service.page}#/crew/background`);
  const crews = page.getByRole("table", { name: "Background crews" });
  await expect(crews).toBeVisible({ timeout: PAGE_READY });
  const row = crews.getByRole("row").filter({ hasText: "Main build" });
  await expect(row).toContainText("Watch · no model · sh -c 'echo boom; exit 1'");
  await expect(row).toContainText("Every 1 hour · raises Needs you");
  await expect(row).toContainText(/Checked .* · exit 1/);
  await row.getByRole("button", { name: "Open the Main build watch" }).click();
  const panel = page.getByRole("region", { name: "Main build watch" });
  await expect(panel.getByRole("heading", { name: "Failing: baseline" })).toBeVisible({ timeout: PAGE_READY });
  await expect(panel.getByText(/A first failing check never raises/)).toBeVisible();
  await expect(panel.getByText("sh -c 'echo boom; exit 1'", { exact: true })).toBeVisible();
  await expect(panel.getByText("Every 1 hour · no network · 5 minutes at most")).toBeVisible();
  await expect(panel.getByText("Raise Needs you")).toBeVisible();
  await expect(panel.getByText("None. A check makes no model call.")).toBeVisible();
  const table = panel.getByRole("table", { name: "Watch changes" });
  expect(await table.getByRole("row").count()).toBe(2);
  const cells = await table.getByRole("row").nth(1).locator('[role="cell"]').allTextContents();
  expect(cells.map((cell) => cell.trim())).toEqual([
    expect.stringMatching(/^\w{3} \d{1,2} \w{3}, \d\d:\d\d$/),
    "Baseline: Failing",
    "1",
    "Recorded. A first check never raises.",
  ]);
  await expect(panel.getByText(/Latest output · the check at .* exit 1/)).toBeVisible();
  await expect(panel.locator('section[aria-label="Latest output"]')).toContainText("boom");
  expect(reads.filter((one) => one.status === 200).map((one) => one.path)).toEqual(
    expect.arrayContaining([`/api/crew/background/${crew.id}/watch`, `/api/crew/background/${crew.id}/watch/changes`]),
  );
});
