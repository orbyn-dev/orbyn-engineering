import { expect, test } from "@playwright/test";

for (const width of [1024, 375]) {
  test(`loading folders stays inside the settings scroller at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 768 });
    await page.goto("/__mock/scenario/default");
    await page.goto("/#/settings/keybindings");
    const nav = page.getByRole("navigation", { name: "Settings sections" });
    await expect(nav.getByRole("link", { name: "Keybindings" })).toBeVisible();
    await nav.getByRole("link", { name: "Maintenance", exact: true }).click();
    await expect(page.locator(".browser")).toBeAttached();
    await nav.getByRole("link", { name: "Keybindings" }).click();
    let release!: () => void;
    const held = new Promise<void>((resolve) => { release = resolve; });
    await page.route("**/api/fs/browse**", async (route) => {
      await held;
      await route.continue();
    });
    try {
      await nav.getByRole("link", { name: "Maintenance", exact: true }).click();
      await expect(page.locator(".browser .sr-only").filter({ hasText: "Loading folders" })).toBeAttached();
      await page.getByRole("button", { name: "Advanced", exact: true }).click();
      await expect(page.locator(".browser")).toBeVisible();
      const main = page.locator("#main");
      await expect.poll(() => main.evaluate((element) => element.scrollHeight - element.clientHeight)).toBe(0);
      await expect(page.locator(".settings-scroll")).toBeVisible();
    } finally {
      release();
    }
  });
}

test("loading Chat preferences do not create an outer scrollbar", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 768 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/settings/keybindings");
  const nav = page.getByRole("navigation", { name: "Settings sections" });
  await expect(nav.getByRole("link", { name: "Keybindings" })).toBeVisible();
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  await page.route("**/api/settings/preferences/query", async (route) => {
    await held;
    await route.continue();
  });
  try {
    await nav.getByRole("link", { name: "Chat", exact: true }).click();
    await expect(page.locator(".settings-scroll .loading .sr-only").filter({ hasText: "Loading settings" }).first()).toBeAttached();
    await expect.poll(() => page.locator("#main").evaluate((element) => element.scrollHeight - element.clientHeight)).toBe(0);
  } finally {
    release();
  }
});
