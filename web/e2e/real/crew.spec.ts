import { expect, test, type Page } from "@playwright/test";
import type { AutopilotModes } from "../../src/generated/AutopilotModes";
import type { CapacityState } from "../../src/generated/CapacityState";
import type { ReviewCoverage } from "../../src/generated/ReviewCoverage";
import { startProject, type Service } from "./service";

/**
 * Crew Preview against the real service, which the mock specs beside it cannot reach. The flag is the service's own
 * startup choice (`ORBYN_CREW_PREVIEW=1`): with it off every Crew route answers 404 and nothing of Crew is drawn, so a
 * run whose service reports it off fails loudly in `beforeAll` rather than skipping.
 *
 * Covered here: Flow renders from the real crew list read; the Needs you inbox renders from its real sources; the
 * Review capacity and coverage screens read their real routes from the Preview hash; and one person action, the
 * Autopilot mode, is saved through Settings and read back from the service.
 *
 * What is not driven through the UI, honestly:
 * - The Review capacity editor is Planned. `LiveSettings` draws its Set a limit and Change the limit buttons disabled
 *   beside a Planned pill and sends no write, so this spec reads the live capacity and coverage screens but cannot save
 *   capacity settings. The person write above is the Autopilot mode instead.
 * - Flow and Needs you have no rows on a fresh service. A task needs a real crew run and an engine (this suite never
 *   starts a live engine), and a held command needs one too, so both surfaces are asserted as the real empty state of
 *   real reads. Their rows stay the component and fixture tests' work until a run can be seeded without an engine.
 * - Approving a held command needs the desktop shell's presence grant, so no approval is attempted.
 */

/** The desktop window less its native title bar, as the other real specs use it. */
const WINDOW = { width: 1280, height: 792 };
/** A fresh dev server compiles the page on its first load. */
const PAGE_READY = 30_000;
const READ_TIMEOUT = 30_000;

let service: Service;
let projectId: string;
/** Whether this service start reports the Crew Preview flag, read from the service itself. */
let preview = false;

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ crewPreview: true }));
  const info = await service.api<{ crew_preview?: boolean }>("GET", "/api/info");
  preview = info.crew_preview === true;
  // A lost crewPreview flag must fail the run, not skip every Crew surface.
  expect(preview).toBe(true);
});

test.afterAll(async () => {
  await service?.stop();
});

/** The one real read for a pathname, started before the navigation that causes it. */
function read(page: Page, pathname: string) {
  return page.waitForResponse(
    (response) => response.request().method() === "GET" && new URL(response.url()).pathname === pathname,
    { timeout: READ_TIMEOUT },
  );
}

/** The answer once it arrives; a status other than 200 is the failure. */
async function answered<T extends { status(): number }>(pending: Promise<T>, pathname: string): Promise<T> {
  const response = await pending;
  expect(response.status(), `${pathname} answered ${response.status()}`).toBe(200);
  return response;
}

test("Flow renders the real, empty crew list", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  const list = read(page, "/api/crew/flow/crews");
  await page.goto(`${service.page}#/crew/flow`);
  const listResponse = await answered(list, "/api/crew/flow/crews");
  expect(((await listResponse.json()) as { crews: unknown[] }).crews).toEqual([]);
  await expect(page.getByRole("heading", { name: "Crew", level: 1 })).toBeVisible({ timeout: PAGE_READY });
  await expect(page.getByRole("tab", { name: "Flow", exact: true })).toHaveAttribute("aria-selected", "true");
  // The real crew list is empty, so the Flow tab says so instead of drawing a task table.
  await expect(page.getByText("No crews running", { exact: true })).toBeVisible();
});

test("Needs you renders its empty inbox from the real sources", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  const reads: { path: string; status: number }[] = [];
  page.on("response", (response) => {
    const path = new URL(response.url()).pathname;
    if (response.request().method() === "GET" && path.startsWith("/api/crew/")) reads.push({ path, status: response.status() });
  });
  await page.goto(`${service.page}#/crew/inbox`);
  const inbox = page.locator('section[aria-label="Needs you"]');
  await expect(inbox.getByText("Nothing needs you", { exact: true })).toBeVisible({ timeout: PAGE_READY });
  await expect(inbox.getByText("When something needs you, it appears here.", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open the Flow view" })).toBeVisible();
  // The inbox stops loading when any source answers, so the others can still be in flight: retry until every Crew read
  // behind it was answered by the real service and the sources are the real ones.
  await expect(async () => {
    expect(reads.filter((entry) => entry.status !== 200)).toEqual([]);
    const paths = new Set(reads.map((entry) => entry.path));
    for (const path of ["/api/crew/approvals/held", "/api/crew/stops", "/api/crew/autopilot/review", "/api/crew/flow/crews", "/api/crew/background"]) {
      expect(paths.has(path), `the inbox read ${path}`).toBe(true);
    }
  }).toPass({ timeout: READ_TIMEOUT });
});

test("Review capacity and coverage read their real routes", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  const capacity = read(page, "/api/crew/capacity");
  await page.goto(`${service.page}#/preview/crew/capacity-live-settings`);
  const capacityResponse = await answered(capacity, "/api/crew/capacity");
  const state = (await capacityResponse.json()) as CapacityState;
  expect(state.project).toBe(projectId);
  expect(state.settings_saved).toBe(false);
  await expect(page.getByRole("heading", { name: /Crew › Review capacity/ })).toBeVisible({ timeout: PAGE_READY });
  await expect(page.getByText("No limit is set.", { exact: true })).toBeVisible();
  await expect(page.getByText("How many changes you can review", { exact: true })).toBeVisible();

  const coverage = read(page, "/api/crew/capacity/coverage");
  await page.goto(`${service.page}#/preview/crew/capacity-live-coverage`);
  const coverageResponse = await answered(coverage, "/api/crew/capacity/coverage");
  const line = (await coverageResponse.json()) as ReviewCoverage;
  expect(line.project).toBe(projectId);
  // Provenance has not begun on this device, so every merge is Not measured and nothing is counted as zero.
  expect(line.agent_lines).toBe(0);
  expect(line.began_unix ?? null).toBeNull();
  await expect(page.getByText("Review coverage", { exact: true })).toBeVisible({ timeout: PAGE_READY });
  await expect(page.getByText("No agent-written lines are recorded yet", { exact: true })).toBeVisible();
});

test("the Autopilot mode a person picks is saved by the service", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  const before = await service.api<AutopilotModes>("GET", "/api/crew/autopilot");
  expect(before.effective_default).toBe("full");
  expect(before.entries.some((entry) => entry.scope === "default")).toBe(false);

  await page.goto(`${service.page}#/settings/crew`);
  const picker = page.getByRole("radiogroup", { name: "Autopilot" });
  await expect(picker).toBeVisible({ timeout: PAGE_READY });
  const write = page.waitForResponse(
    (response) => response.request().method() === "PUT" && new URL(response.url()).pathname === "/api/crew/autopilot/mode",
    { timeout: READ_TIMEOUT },
  );
  await picker.getByRole("radio", { name: /^On\b/ }).click();
  expect((await write).status(), "PUT /api/crew/autopilot/mode").toBe(200);
  // The write reached the service: its own read of the saved modes now names the default the person chose.
  await expect
    .poll(async () => (await service.api<AutopilotModes>("GET", "/api/crew/autopilot")).entries.find((entry) => entry.scope === "default")?.mode)
    .toBe("on");

  // A reload reads that saved choice back from the service, not from the page's optimistic state.
  await page.reload();
  await expect(page.getByRole("radiogroup", { name: "Autopilot" }).getByRole("radio", { name: /^On\b/ })).toHaveAttribute("aria-checked", "true", { timeout: PAGE_READY });
});
