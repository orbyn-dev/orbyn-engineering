import { expect, test } from "@playwright/test";

/**
 * The reading layout once rendered a blank page with only the title bar: its main area was placed in a second grid
 * row that no longer exists, so it collapsed to its content. The sidebar is folded away there, the thread is one
 * column, and the page must fill the window. Runs in WebKit too when ORBYN_E2E_WEBKIT=1 (the app's web view).
 */
const THEMES = [
  ["light", { theme: "light" }],
  ["dark", { theme: "dark", darkPreset: null }],
  ["dim", { theme: "dark", darkPreset: "dim" }],
] as const;

for (const [name, theme] of THEMES) {
  test(`the reading layout fills the window and shows its thread, composer and no sidebar (${name})`, async ({ page }) => {
    await page.addInitScript(
      (prefs) => localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ workspaceLayout: "reading", sidebarCollapsed: true, ...prefs })),
      theme,
    );
    await page.setViewportSize({ width: 1280, height: 820 });
    await page.goto("/__mock/scenario/default");

    for (const hash of ["#/threads/t-done", "#/new"]) {
      await page.goto(`/${hash}`);
      const main = page.locator(".main");
      await expect(main).toBeVisible();
      // One row, from the window's top edge to its bottom edge, not a content-sized strip below a gap.
      await expect.poll(async () => (await main.boundingBox())?.y).toBe(0);
      await expect.poll(async () => Math.round((await main.boundingBox())?.height ?? 0)).toBe(820);
      await expect(page.getByRole("textbox", { name: /^Message|Ask anything/ }).first()).toBeVisible();
      await expect(page.getByRole("navigation", { name: "Workspace" })).toBeHidden();
      expect(await page.evaluate(() => document.body.innerText.length), hash).toBeGreaterThan(150);
    }
    await page.goto("/#/threads/t-done");
    await expect(page.getByRole("heading", { name: "Explain how the event log resumes" })).toBeVisible();
  });
}
