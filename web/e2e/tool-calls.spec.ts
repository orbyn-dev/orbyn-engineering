import { expect, test } from "@playwright/test";

/** What jsdom cannot see: where a capped output starts, in a real layout. */
test("a long output opens at its first line, not its end", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 800 });
  await page.goto("/#/threads/t-tools");
  await page.locator("button.work-summary").first().click();
  // Open every row; groups first, then what they hold.
  await page.evaluate(async () => {
    for (let pass = 0; pass < 3; pass++) {
      document.querySelectorAll<HTMLElement>('button.tcr[aria-expanded="false"]').forEach((row) => row.click());
      await new Promise((resolve) => setTimeout(resolve, 300));
    }
  });
  const block = page.locator(".co.cap").first();
  await block.scrollIntoViewIfNeeded();
  expect(await block.evaluate((element) => element.scrollTop)).toBe(0);
  await expect(page.getByText("case-01.spec.ts").first()).toBeInViewport();
  await expect(page.getByRole("button", { name: "Show all 214 lines" })).toBeVisible();
});
