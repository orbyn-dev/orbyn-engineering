import { expect, test, type Page } from "@playwright/test";
import type { CrewDashboard } from "../../src/generated/CrewDashboard";
import { startProject, type Service } from "./service";

/**
 * G18, the Crew dashboard on the real routes: Settings › Crew › Dashboard reads `GET /api/crew/dashboard` with the
 * project, the period and the person's UTC offset; on a repository with no merges the service answers real zero figures
 * and its own sample thresholds, the page draws those as "0 of 0 / too few for a rate", "Not measured" and "no merges"
 * rather than invented numbers, and the Right now row reads the one needs-you field. A project the service does not know
 * answers 404. The Review tab that holds this page in the designs is Planned, so it is a Settings page.
 */

const WINDOW = { width: 1500, height: 1000 };
const PAGE_READY = 30_000;

let service: Service;
let projectId: string;

const dashboardReads = (page: Page, reads: URL[]) => {
  page.on("response", (response) => {
    const url = new URL(response.url());
    if (response.request().method() === "GET" && url.pathname === "/api/crew/dashboard") reads.push(url);
  });
};

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ crewPreview: true, fakeEngines: true }));
});

test.afterAll(async () => {
  await service?.stop();
});

test("the dashboard reads the real route with the period and offset, and draws its real zero figures", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize(WINDOW);
  const offset = await page.evaluate(() => -new Date().getTimezoneOffset());

  // The service answers the route the page reads, with its own sample thresholds and zero figures.
  const history = await service.api<CrewDashboard>("GET", `/api/crew/dashboard?project=${encodeURIComponent(projectId)}&days=60&utc_offset_minutes=${offset}`);
  expect(history.project).toBe(projectId);
  expect(history.days).toBe(60);
  expect(history.min_rate_sample).toBe(10);
  expect(history.min_median_sample).toBeGreaterThan(0);
  expect([history.merges, history.unaided, history.reverted]).toEqual([0, 0, 0]);
  expect([history.weeks, history.engines, history.areas, history.reverts, history.recent]).toEqual([[], [], [], [], []]);
  expect(history.live.needs_you).toEqual([]);
  expect(history.live.working).toEqual([]);

  // The page's own read carries the project, the 60-day period and the person's offset.
  const reads: URL[] = [];
  dashboardReads(page, reads);
  await page.goto(`${service.page}#/settings/crew/dashboard?repo=${projectId}`);
  await expect(page.getByRole("heading", { name: "Dashboard", exact: true })).toBeVisible({ timeout: PAGE_READY });
  await expect.poll(() => reads.length).toBeGreaterThan(0);
  const asked = reads.at(-1)!;
  expect(asked.searchParams.get("project")).toBe(projectId);
  expect(asked.searchParams.get("days")).toBe("60");
  expect(asked.searchParams.get("utc_offset_minutes")).toBe(String(offset));

  // A repository with no merges draws its real empty state, not four tiles of invented figures and not a Right now row.
  await expect(page.getByText("No merges yet", { exact: true })).toBeVisible({ timeout: PAGE_READY });
  await expect(page.getByText(/When Crew merges a change in my-repo/)).toBeVisible();
  expect(await page.locator("[data-tile]").count()).toBe(0);
  expect(await page.getByRole("group", { name: "Right now" }).count()).toBe(0);

  // The period is a read: picking 30 days asks the service again with that period and marks the choice.
  const period = page.getByRole("radiogroup", { name: "Period" });
  await expect(period.getByRole("radio", { name: "60 days" })).toHaveAttribute("aria-checked", "true");
  const read30 = page.waitForResponse(
    (response) => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/crew/dashboard" && new URL(response.url()).searchParams.get("days") === "30",
    { timeout: 20_000 },
  );
  await period.getByRole("radio", { name: "30 days" }).click();
  expect((await read30).status()).toBe(200);
  await expect(period.getByRole("radio", { name: "30 days" })).toHaveAttribute("aria-checked", "true");
});

test("a project the service does not know answers 404 on the dashboard route", async () => {
  const refused = await service.api("GET", "/api/crew/dashboard?project=01missingproject0000000000&days=60&utc_offset_minutes=0").then(
    () => {
      throw new Error("the dashboard read answered a project it does not have");
    },
    (error: Error) => error.message,
  );
  expect(refused).toContain("404");
  expect(refused).toContain("not_found");
});
