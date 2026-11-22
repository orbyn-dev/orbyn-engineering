import { expect, test } from "@playwright/test";

for (const width of [1280, 1920]) {
  test(`provider table columns align at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/__mock/scenario/default", { waitUntil: "domcontentloaded" });
    await page.goto("/#/settings/providers", { waitUntil: "domcontentloaded" });
    const table = page.getByRole("table", { name: "Providers" });
    await expect(table).toBeVisible();
    const rows = table.locator(".set-tr");
    await expect.poll(() => rows.count()).toBeGreaterThan(2);
    await expect(table.getByText("Signed out", { exact: true }).first()).toBeVisible();
    await expect(table.getByText("Ready", { exact: true }).first()).toBeVisible();
    const pill = table.locator(".set-states .set-pill").first();
    await pill.evaluate((element) => { element.textContent = `Update ${"1234567890".repeat(5)}`; });
    await expect.poll(() => pill.evaluate((element) => element.scrollWidth <= element.clientWidth + 1 && element.scrollHeight <= element.clientHeight + 1)).toBe(true);
    const positions = await rows.evaluateAll((elements) => elements.map((row) => {
      const cells = Array.from(row.children);
      return { text: row.textContent, positions: [2, 3].map((index) => cells[index]?.getBoundingClientRect().left ?? -1) };
    }));
    const clipped = await table.locator(".set-states").evaluateAll((cells) => cells.some((cell) => cell.scrollWidth > cell.clientWidth + 1 || cell.scrollHeight > cell.clientHeight + 1));
    expect(clipped).toBe(false);
    const header = positions[0]!;
    for (const row of positions.slice(1)) {
      expect(Math.abs(row.positions[0]! - header.positions[0]!)).toBeLessThanOrEqual(1);
      expect(Math.abs(row.positions[1]! - header.positions[1]!)).toBeLessThanOrEqual(1);
    }
  });
}
