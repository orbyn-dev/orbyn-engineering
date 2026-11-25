import { expect, test, type Page } from "@playwright/test";
import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { startProject, type Service } from "./service";

/**
 * The engine host's own actions on the live service: a real `orbyn-engine-host` process is started, stopped,
 * restarted and tried again through the routes the screens call, and the screens read the results back.
 * Build this checkout's host with `cd engine-host && npm ci && node scripts/build-sea.mjs`, or set
 * ORBYN_ENGINE_HOST to an existing one. No engine login is needed: the process starts and handshakes on its own.
 */

const REPO = resolve(import.meta.dirname, "../../..");
const EXE = process.platform === "win32" ? "orbyn-engine-host.exe" : "orbyn-engine-host";
// Absolute: the service starts the host from another working directory, so a relative override would not be found.
const HOST = resolve(process.env.ORBYN_ENGINE_HOST ?? join(REPO, "engine-host/dist", EXE));
const BUILD = `Build the engine host first: cd engine-host && npm ci && node scripts/build-sea.mjs, or set ORBYN_ENGINE_HOST (looked for ${HOST})`;

/**
 * A missing host build is a skip with an annotation naming the CI job that will build it (#1588), so the run says it did not run.
 * With ORBYN_REQUIRE_SEA_HOST=1 (set where the build is expected) it is a failure instead, so the spec can never read green unrun.
 */
const needHost = () => {
  if (existsSync(HOST)) return;
  if (process.env.ORBYN_REQUIRE_SEA_HOST === "1") throw new Error(`ORBYN_REQUIRE_SEA_HOST=1 but there is no engine host. ${BUILD}`);
  test.info().annotations.push({ type: "needs-sea-host", description: "Skipped: no engine host build; CI does not build it yet (#1588)" });
  test.skip(true, BUILD);
};

type Host = { family: string; state: string; host_version?: string; last_crash?: { error_kind: string } | null };

const hostOf = async (service: Service, family = "claude"): Promise<Host | undefined> =>
  (await service.api<Host[]>("GET", "/api/engine-host")).find((row) => row.family === family);

const postTo = (page: Page, path: string) =>
  page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === path);

test("stops and restarts a running engine host against the live service", async ({ page }) => {
  needHost();
  test.setTimeout(90_000);
  const { service } = await startProject({ fakeEngines: true, engineHost: { program: HOST } });
  try {
    // A host starts on a run or on Restart; no screen offers another way to start an idle one.
    const started = await service.api<Host>("POST", "/api/engine-host/claude/restart", { stop_background: false });
    expect(started.state).toBe("running");
    // The host names its own version; a release bump must not need a test edit.
    expect(started.host_version).toMatch(/^\d+\.\d+\.\d+/);

    await page.goto(`${service.page}#/settings/providers`);
    const providers = page.getByRole("table", { name: "Engine host" });
    await expect(providers).toBeVisible();
    const claude = providers.getByRole("row").filter({ hasText: "Claude Code" });
    await expect(claude.getByText("Running", { exact: true })).toBeVisible();
    await expect(claude).toContainText("One host for every Claude Code thread");
    await expect(claude.getByRole("button", { name: "Try again" })).toHaveCount(0);

    // Maintenance › Processes draws the live host, with its two person actions.
    await page.goto(`${service.page}#/settings/maintenance`);
    const processes = page.getByRole("list", { name: "Processes" });
    const row = processes.getByRole("listitem").filter({ hasText: "Claude Code engine host" });
    await expect(row).toBeVisible({ timeout: 20_000 });
    await expect(row).toContainText("shared by every Claude Code thread");

    // Restart: one POST, answered with a host it started again.
    const restarted = postTo(page, "/api/engine-host/claude/restart");
    await row.getByRole("button", { name: "Restart Claude Code engine host" }).click();
    const restartedAnswer = await restarted;
    expect(restartedAnswer.status()).toBe(200);
    expect(await restartedAnswer.json()).toMatchObject({ family: "claude", state: "running" });
    await expect.poll(async () => (await hostOf(service))?.state).toBe("running");

    // Stop asks in the confirm frame first; the answer ends the live process and the row goes.
    await row.getByRole("button", { name: "Stop Claude Code engine host" }).click();
    const question = page.getByRole("dialog", { name: "Stop the Claude Code engine host?" });
    await expect(question).toBeVisible();
    await expect(question).toContainText("It stops now.");
    const stopped = postTo(page, "/api/engine-host/claude/stop");
    await question.getByRole("button", { name: "Stop", exact: true }).click();
    const stoppedAnswer = await stopped;
    expect(stoppedAnswer.status()).toBe(200);
    expect(stoppedAnswer.request().postDataJSON()).toEqual({ stop_background: false });
    await expect.poll(async () => (await hostOf(service))?.state).toBe("stopped");
    await expect(row).toHaveCount(0);

    // The Providers group reads the same status: the host is stopped, and not a process.
    await page.goto(`${service.page}#/settings/providers`);
    const stoppedRow = page.getByRole("table", { name: "Engine host" }).getByRole("row").filter({ hasText: "Claude Code" });
    await expect(stoppedRow.getByText("Stopped", { exact: true })).toBeVisible();
  } finally {
    await service.stop();
  }
});

test("tries a host again after it gave up, and a fixed fault starts it", async ({ page }) => {
  needHost();
  test.setTimeout(180_000);
  const folder = mkdtempSync(join(tmpdir(), "orbyn-host-fault-"));
  const program = join(folder, EXE);
  try {
    const { service } = await startProject({ fakeEngines: true, engineHost: { program } });
    try {
      // There is no program, so each start fails; the service tries five times with backoff, then gives up.
      const first = await service.api<Host>("POST", "/api/engine-host/claude/restart", { stop_background: false });
      expect(first).toMatchObject({ state: "restarting", last_crash: { error_kind: "start_failed" } });
      await expect.poll(async () => (await hostOf(service))?.state, { timeout: 120_000, intervals: [1000] }).toBe("cant_start");
      expect((await hostOf(service))?.last_crash?.error_kind).toBe("start_failed");

      // The fault is fixed, and the person's Try again starts it: the restart route, from the row.
      copyFileSync(HOST, program);
      await page.goto(`${service.page}#/settings/providers`);
      const claude = page.getByRole("table", { name: "Engine host" }).getByRole("row").filter({ hasText: "Claude Code" });
      await expect(claude.getByText("Can’t start", { exact: true })).toBeVisible();
      await expect(claude).toContainText("The engine host is unavailable after repeated failures");
      const retried = postTo(page, "/api/engine-host/claude/restart");
      await claude.getByRole("button", { name: "Try again" }).click();
      const answer = await retried;
      expect(answer.status()).toBe(200);
      const restarted = (await answer.json()) as Host;
      expect(restarted).toMatchObject({ family: "claude", state: "running" });
      expect(restarted.host_version).toMatch(/^\d+\.\d+\.\d+/);
      await expect.poll(async () => (await hostOf(service))?.state).toBe("running");
      await expect(claude.getByText("Running", { exact: true })).toBeVisible();
      await expect(claude.getByRole("button", { name: "Try again" })).toHaveCount(0);
    } finally {
      // The service stops the host it started before the copied program can be removed on Windows.
      await service.stop();
    }
  } finally {
    rmSync(folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
  }
});
