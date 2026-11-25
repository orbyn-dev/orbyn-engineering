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
 * The project bind route exists and is Person-class; the last test binds and unbinds a stored connection's scope, and reads
 * the candidates route and the update and state writes now served for GitHub (#1593) through the stand-in `gh`. The four
 * token trackers keep update, state and candidates Planned (422 unsupported) and their connects need a keychain token, so
 * they are not exercised here (an owner action).
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

test("the project bind route binds and unbinds a stored scope, and the GitHub edit routes are served", async () => {
  const failure = async (method: string, path: string, body?: unknown) => {
    try {
      await service.api(method, path, body);
      return "";
    } catch (error) {
      return (error as Error).message;
    }
  };
  // The bind route exists (docs/issues.md). GitHub is connected through the stand-in gh, but its connection does not name
  // this scope yet, so the bind is refused 400 invalid before anything is stored.
  const refused = await failure("POST", `/api/projects/${projectId}/issue-tracker`, { tracker: "github", scope: "o/r" });
  expect(refused).toContain("400");
  expect(refused).toContain("invalid");
  // With the connection's own scope stored, the real Person route binds (200) and unbinds (204), idempotently.
  await service.api("POST", "/api/issues/trackers/github/connect", { scope: "o/r" });
  const bound = await service.api<{ tracker: string; scope: string }>(
    "POST",
    `/api/projects/${projectId}/issue-tracker`,
    { tracker: "github", scope: "o/r" },
  );
  expect(bound).toEqual({ tracker: "github", scope: "o/r" });
  expect(await service.api("POST", `/api/projects/${projectId}/issue-tracker`, { tracker: null })).toBeUndefined();
  expect(await service.api("POST", `/api/projects/${projectId}/issue-tracker`, { tracker: null })).toBeUndefined();
  // The issue-edit routes are served for GitHub (#1593). The candidates read answers the stand-in's labels and assignable
  // people; an update writes the present fields; one state write closes and one reopens, keeping GitHub's own close reason.
  expect(await service.api("GET", "/api/issues/candidates?tracker=github&scope=o%2Fr")).toEqual({
    labels: [{ name: "bug", color: null }, { name: "docs", color: null }],
    assignees: [{ login: "ada", name: null, id: "ada" }, { login: "grace", name: null, id: "grace" }],
    truncated: false,
  });
  const updated = await service.api<{ tracker: string; scope: string; key: string; body: string }>(
    "POST",
    "/api/issues/item/update",
    { tracker: "github", scope: "o/r", key: "12", body: "Re-read on the stand-in." },
  );
  expect(updated).toMatchObject({ tracker: "github", scope: "o/r", key: "12", body: "Re-read on the stand-in." });
  const closed = await service.api<{ state: { category: string; name: string; close_reason: string | null } }>(
    "POST",
    "/api/issues/item/state",
    { tracker: "github", scope: "o/r", key: "12", state: "Closed" },
  );
  expect(closed.state).toMatchObject({ category: "closed", name: "Closed", close_reason: "completed" });
  const reopened = await service.api<{ state: { category: string; name: string } }>(
    "POST",
    "/api/issues/item/state",
    { tracker: "github", scope: "o/r", key: "12", state: "Open" },
  );
  expect(reopened.state).toMatchObject({ category: "open", name: "Open" });
  // Nothing to change is the route's own 400, refused before the tracker is asked; a tracker whose edit is not built and is
  // not connected in this fixture is its 409, not a guessed 422 (the four token connects need a keychain token: an owner action).
  expect(await failure("POST", "/api/issues/item/update", { tracker: "github", scope: "o/r", key: "12" })).toContain("400");
  expect(await failure("GET", "/api/issues/candidates?tracker=gitlab&scope=o%2Fr")).toContain("409");
});
