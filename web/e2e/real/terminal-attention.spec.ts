import { expect, test } from "@playwright/test";
import { send, settled, startProject } from "./service";

/**
 * CLI-agent attention (W10, docs/cli-agent-attention.md) against a real orbynd: the shell stream's `thread_event` notice makes the
 * window follow that thread's log and raise the toast (In-app notifications), and a window that opens later reads the thread's open
 * items once when the thread's terminal opens, drawing the same item with no notice.
 */

test("a waiting engine console raises the notice toast, and a later window draws the item from the open-items read", async ({ page }) => {
  test.setTimeout(180_000);
  const { service, projectId } = await startProject({ fakeEngines: true });
  try {
    // The toast follows In-app notifications (a saved window preference), and the shortest quiet time the service allows.
    await page.addInitScript(() => localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ inAppNotifications: true })));
    await service.api("PUT", "/api/preferences", { values: { terminal_attention_quiet_seconds: 10 } });
    const thread = await service.api<{ id: string }>("POST", "/api/threads", { project_id: projectId, engine: "claude", title: "Attention source" });
    const elsewhere = await service.api<{ id: string }>("POST", "/api/threads", { project_id: projectId, engine: "claude", title: "Elsewhere" });
    await page.setViewportSize({ width: 1400, height: 900 });
    await page.goto(`${service.page}#/threads/${thread.id}`);

    // A session for the engine console to resume, then the console itself on the fake engine.
    await send(page, "hello");
    await settled(service, thread.id);
    await page.getByRole("button", { name: "Terminal", exact: true }).click();
    await page.getByRole("button", { name: "Open engine console", exact: true }).click();
    const drawer = page.locator(".terminal-drawer-body");
    await expect(page.getByRole("tab", { name: "Claude console" })).toHaveAttribute("aria-selected", "true");
    await expect(drawer.locator(".xterm-rows > div").filter({ hasText: "fake console on claude-session-1" }).first()).toBeVisible({ timeout: 20_000 });

    // The console prints a question-shaped last line, then goes quiet: the service detects a wait after the quiet time.
    await drawer.locator(".xterm-helper-textarea").first().focus();
    await page.keyboard.type("Overwrite auth/redirect.ts? (y/N)\n");
    await expect(drawer.locator(".xterm-rows > div").filter({ hasText: "console heard Overwrite auth/redirect.ts" }).first()).toBeVisible();

    // The waiting thread is not on screen, so the notice is the toast the frames draw (In-app notifications).
    await page.evaluate((hash) => { location.hash = hash; }, `#/threads/${elsewhere.id}`);
    await expect(page).toHaveURL(new RegExp(elsewhere.id));
    await expect(page.getByText(/Claude Code needs you in “Claude console”/)).toBeVisible({ timeout: 40_000 });
    // The service's open-items read holds the wait it announced.
    await expect
      .poll(async () => (await service.api<{ items: unknown[] }>("GET", `/api/threads/${thread.id}/terminal-attention`)).items.length, { timeout: 40_000 })
      .toBe(1);

    // A later window has no notice to follow; opening the thread's terminal makes it read the open items once.
    const fresh = await page.context().newPage();
    await fresh.setViewportSize({ width: 1400, height: 900 });
    await fresh.addInitScript(() => localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ inAppNotifications: true, terminalOpen: false })));
    await fresh.goto(`${service.page}#/threads/${thread.id}`);
    const region = fresh.getByRole("region", { name: "Terminal agents waiting for you" });
    await expect(fresh.getByRole("button", { name: "Terminal", exact: true })).toBeVisible({ timeout: 30_000 });
    await expect(region).toHaveCount(0);
    const read = fresh.waitForResponse((response) => response.request().method() === "GET" && new URL(response.url()).pathname === `/api/threads/${thread.id}/terminal-attention`);
    await fresh.getByRole("button", { name: "Terminal", exact: true }).click();
    const response = await read;
    expect(response.status()).toBe(200);
    const body = (await response.json()) as { items: Array<Record<string, unknown>> };
    expect(body.items).toHaveLength(1);
    expect(body.items[0]).toMatchObject({ v: 1, op: "waiting", engine: "claude", kind: "yes_no", terminal_title: "Claude console" });
    expect(String(body.items[0]?.["excerpt"])).toContain("Overwrite auth/redirect.ts");
    await expect(region).toContainText("Claude Code in the terminal “Claude console” is waiting for an answer.");
    // The silence strip under the tab row draws the same read item for the console on screen, with no action.
    await fresh.getByRole("tab", { name: /Claude console/ }).click();
    await expect(fresh.locator(".term-wait")).toContainText("Waiting for you · Claude Code has been silent for");
    await expect(fresh.locator(".term-wait")).not.toContainText("Answer here");
    await fresh.close();
  } finally {
    await service.stop();
  }
});
