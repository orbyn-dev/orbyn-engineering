import { expect, test } from "@playwright/test";

/** The bar's buttons and every page's heading share one centre line, 22px from the top of the window. */
test("the page heading sits on the title bar's centre line", async ({ page }) => {
  await page.goto("/__mock/scenario/default");
  for (const hash of ["#/threads/t-done", "#/pulls", "#/usage", "#/tasks"]) {
    await page.goto(`/${hash}`);
    const heading = page.locator(".main h1").first();
    await expect(heading).toBeVisible();
    const centre = async (selector: string) => {
      const box = await page.locator(selector).first().boundingBox();
      return box ? box.y + box.height / 2 : NaN;
    };
    const bar = await centre(".titlebar .tbi");
    const title = await heading.boundingBox();
    expect(bar, hash).toBeCloseTo(22, 0);
    expect((title?.y ?? 0) + (title?.height ?? 0) / 2, hash).toBeCloseTo(bar, 0);
  }
});
