import { expect, test } from "@playwright/test";
import { statfsSync } from "node:fs";
import type { BackgroundCrew } from "../../src/generated/BackgroundCrew";
import type { BackgroundRunDetail } from "../../src/generated/BackgroundRunDetail";
import type { BackgroundRunList } from "../../src/generated/BackgroundRunList";
import { startProject, type Service } from "./service";

/**
 * KC4 (M24), the disk guard, against the real service: the person sets the floor in Settings › Crew, so the
 * floor is person-settable; a background crew's start under it is refused with the free space and the floor in
 * the reason, and the refusal keeps one stopped history row (`reason_code` disk_low, no stop reason, nothing
 * spent) that the Background page draws as "Not started · Disk" with the service's own sentence.
 *
 * The guard reads the volume that holds the lead's folder. The fixture project must sit on a volume smaller than
 * the floor: the build box's `/tmp` is a 24 GB tmpfs and the floor here is its maximum 500 GB, so it refuses.
 * The spec checks the volume first and fails with its own message rather than pretending elsewhere.
 */

const FLOOR_GB = 500;
const GIB = 1024 ** 3;
const WINDOW = { width: 1280, height: 792 };
const PAGE_READY = 30_000;
const SENTENCE = new RegExp(`Not started: [\\d.]+ (GB|MB) free, Crew needs ${FLOOR_GB} GB\\. Free space and try again\\.`);

let service: Service;
let projectId: string;

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ crewPreview: true, fakeEngines: true }));
});

test.afterAll(async () => {
  await service?.stop();
});

test("a person's disk floor refuses a background start with the free space in the reason", async ({ page }) => {
  test.setTimeout(180_000);
  await page.setViewportSize(WINDOW);

  // The person sets the floor through Settings › Crew; the service holds it and answers the read.
  await page.goto(`${service.page}#/settings/crew`);
  const field = page.getByRole("textbox", { name: "Free disk space Crew needs" });
  await expect(field).toBeVisible({ timeout: PAGE_READY });
  await field.fill(String(FLOOR_GB));
  const saved = page.waitForResponse(
    (response) => response.request().method() === "PUT" && new URL(response.url()).pathname === "/api/settings/preferences",
    { timeout: PAGE_READY },
  );
  await page.locator('section[aria-label="Free disk space Crew needs"]').getByRole("button", { name: "Save", exact: true }).click();
  expect((await saved).status(), "PUT /api/settings/preferences").toBe(200);
  await expect(page.getByText("Saved", { exact: true })).toBeVisible();
  const preferences = await service.api<{ key: string; value: unknown }[]>("GET", "/api/preferences");
  expect(preferences.find((row) => row.key === "crew_min_free_gb")?.value).toBe(FLOOR_GB);

  // A volume smaller than the floor, so the guard can refuse: the box's /tmp tmpfs.
  const volume = statfsSync(service.repo);
  const freeBytes = Number(volume.bavail) * Number(volume.bsize);
  expect(
    freeBytes,
    `the fixture project's volume holds ${(freeBytes / GIB).toFixed(1)} GB free; a ${FLOOR_GB} GB floor needs a smaller volume to refuse`,
  ).toBeLessThan(FLOOR_GB * GIB);

  // A background crew that is on, and its one start by hand: the disk guard refuses it.
  const crew = await service.api<BackgroundCrew>("POST", "/api/crew/background/create", {
    project_id: projectId,
    name: "Nightly build",
    template: "fix_failing_ci",
    trigger: { kind: "schedule", every_hours: 24 },
    budget: { turns: 10, minutes: 30 },
    daily: { runs: 1, turns: 10 },
    utc_offset_minutes: 0,
  });
  expect((await service.api<BackgroundCrew>("POST", `/api/crew/background/${crew.id}/enable`, {})).enabled).toBe(true);
  const refusal = await service.api("POST", `/api/crew/background/${crew.id}/run-now`, {}).then(
    () => {
      throw new Error("Run now was not refused: the disk guard let the start through");
    },
    (error: Error) => error.message,
  );
  expect(refusal).toContain('"code":"policy_denied"');
  expect(refusal).toMatch(SENTENCE);

  // The refusal is the crew's history: one stopped run with the code and the sentence, nothing spent, nobody waiting.
  const listed = await service.api<BackgroundRunList>("GET", `/api/crew/background/${crew.id}/runs?offset=0&limit=10`);
  expect(listed.total).toBe(1);
  const run = listed.runs[0]!;
  expect(run.state).toBe("stopped");
  expect(run.stop_reason ?? null).toBeNull();
  expect(run.reason_code).toBe("disk_low");
  expect(run.review_waiting).toBe(false);
  expect([run.turns_spent, run.minutes_spent]).toEqual([0, 0]);
  expect(run.review_reason).toMatch(SENTENCE);
  const detail = await service.api<BackgroundRunDetail>("GET", `/api/crew/background/runs/${run.id}`);
  expect(detail.run.id).toBe(run.id);
  expect(detail.crew_id).toBe(crew.id);
  expect(detail.name).toBe("Nightly build");
  expect(detail.template).toBe("fix_failing_ci");

  // The web draws that history row from the real read: the chip and the service's own sentence with the free space.
  await page.goto(`${service.page}#/crew/background`);
  const runs = page.getByRole("table", { name: "Recent runs" });
  await expect(runs).toBeVisible({ timeout: PAGE_READY });
  const row = runs.getByRole("row").filter({ hasText: "Nightly build" });
  await expect(row).toContainText("Not started · Disk");
  await expect(row).toContainText(SENTENCE);
  await expect(row).toContainText("No worker ran. Nothing was removed to make room.");
});
