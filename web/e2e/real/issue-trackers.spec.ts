import { expect, test } from "@playwright/test";
import { startProject, type Service } from "./service";

// The first test reads the trackers before any connection is stored; a later test stores one, and the next clears it.
test.describe.configure({ mode: "serial" });

/**
 * F67: issue trackers through a real `orbynd` with `ORBYN_ISSUES=1` and a stand-in `gh`. The tracker is GitHub through the
 * person's own `gh` sign-in: the real `gh` program on the fixture PATH is the stand-in, so no real GitHub is contacted and no
 * keychain is touched. Also F3's observation of Settings › Source control: the Hosts table and its Rescan button, the rows it
 * draws, and no local control to turn a source-control host on or off.
 *
 * The project bind route and the issue-write routes are not on this service; the last test records them as absent.
 */

const WINDOW = { width: 1500, height: 1000 };

let service: Service;
let projectId = "";

interface TrackerRow {
  tracker: string;
  connected: boolean;
  scope?: string | null;
  host?: string | null;
  detail?: string | null;
}

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ forge: true, issues: true }));
});
test.afterAll(() => service.stop());
test.beforeEach(async ({ page }) => {
  await page.setViewportSize(WINDOW);
});

test("the trackers route lists every tracker and GitHub is connected through gh", async () => {
  const rows = await service.api<TrackerRow[]>("GET", "/api/issues/trackers");
  expect(rows.map((row) => row.tracker)).toEqual(["github", "gitlab", "azure_devops", "forgejo", "linear"]);
  // GitHub reads through the person's own gh sign-in: the stand-in `gh` on PATH is installed, so it is connected with no stored
  // connection of its own and no token anywhere.
  expect(rows[0]).toMatchObject({ connected: true, scope: null, host: null });
  expect(rows.slice(1).every((row) => !row.connected)).toBe(true);
});

test("Settings › Source control draws the Issue trackers group and the Hosts table, and offers no host on/off control", async ({ page }) => {
  await page.goto(`${service.page}#/settings/source-control`);
  await expect(page.getByText("Issue trackers", { exact: false }).first()).toBeVisible({ timeout: 30_000 });
  const trackers = page.getByRole("table", { name: "Issue trackers" });
  await expect(trackers).toBeVisible();
  const github = trackers.getByRole("row").filter({ hasText: "GitHub" });
  await expect(github).toContainText("Signed in");
  await expect(github).toContainText("Uses the gh sign-in on this computer");
  // The four token trackers are listed as not connected, each with the one way to connect it.
  for (const name of ["GitLab", "Azure DevOps", "Forgejo", "Linear"]) {
    const row = trackers.getByRole("row").filter({ hasText: name });
    await expect(row).toContainText("Not connected");
    await expect(row.getByRole("button", { name: "Add token…" })).toBeVisible();
  }

  // The connection's own scope, stored by the real connect route, is the line under the tracker.
  await service.api("POST", "/api/issues/trackers/github/connect", { scope: "o/r" });
  await page.reload();
  await expect(trackers.getByRole("row").filter({ hasText: "GitHub" })).toContainText("o/r");

  // F3: Settings › Source control is the writing style, the Automatic pull preference and a Hosts table with a Rescan button.
  await expect(page.getByLabel("Source control writing style")).toBeVisible();
  await expect(page.getByRole("switch", { name: "Automatically pull" })).toBeVisible();
  const hosts = page.getByRole("table", { name: "Hosts" });
  await expect(hosts).toBeVisible();
  await expect(hosts.getByRole("row").filter({ hasText: "GitHub" })).toContainText("Signed in");
  await expect(page.getByRole("button", { name: "Rescan" })).toBeVisible();
  // The page's switches belong to preferences (Automatically pull); no source-control host has a local on/off control, and no
  // host row carries an account picker or a switch, toggle, button, checkbox or radio of its own.
  expect(await page.getByRole("switch").count()).toBeGreaterThan(0);
  await expect(hosts.getByRole("switch")).toHaveCount(0);
  await expect(hosts.getByRole("button")).toHaveCount(0);
  await expect(hosts.getByRole("combobox")).toHaveCount(0);
  await expect(hosts.getByRole("checkbox")).toHaveCount(0);
  await expect(hosts.getByRole("radio")).toHaveCount(0);
});

test("connect and disconnect are the real Person routes, and a stored scope comes back on the row", async () => {
  const connected = await service.api<TrackerRow>("POST", "/api/issues/trackers/github/connect", { scope: "o/r" });
  expect(connected).toMatchObject({ tracker: "github", connected: true, scope: "o/r", host: null });
  expect(JSON.stringify(connected)).not.toContain("token");
  const disconnected = await service.api<TrackerRow>("POST", "/api/issues/trackers/github/disconnect");
  expect(disconnected).toMatchObject({ tracker: "github", connected: true, scope: null });
  // A refused connect names what it will not do; GitHub takes no host and no token here.
  const refusal = await service.api("POST", "/api/issues/trackers/github/connect", { token: "ghp_not_a_real_token" }).then(
    () => "",
    (failure: Error) => failure.message,
  );
  expect(refusal).toContain("400");
  expect(refusal).toContain("GitHub reads through your own gh");
});

test("the project bind, candidates and the update and state writes are not on this service", async () => {
  const missing = async (method: string, path: string, body?: unknown) => {
    try {
      await service.api(method, path, body);
      return "";
    } catch (failure) {
      return (failure as Error).message;
    }
  };
  // docs/issues.md binds a tracker to a project through this route; the contract is written and the route is not built.
  expect(await missing("POST", `/api/projects/${projectId}/issue-tracker`, { tracker: "github", scope: "o/r" })).toContain("404");
  // The candidates read and the update and state writes are not on this service (PR #1521 served create, comment and react,
  // which the web client does not call), so the edit controls have no route to use.
  expect(await missing("GET", "/api/issues/candidates?tracker=github&scope=o%2Fr")).toContain("404");
  expect(await missing("POST", "/api/issues/item/update", { tracker: "github", scope: "o/r", key: "7", title: "x" })).toContain("404");
  expect(await missing("POST", "/api/issues/item/state", { tracker: "github", scope: "o/r", key: "7", state: "Open" })).toContain("404");
});
