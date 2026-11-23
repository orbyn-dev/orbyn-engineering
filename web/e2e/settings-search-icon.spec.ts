import { expect, test } from "@playwright/test";

test("settings search icon retains its square size", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 800 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/settings/general");
  const icon = page.locator(".settings-search > svg");
  await expect(icon).toBeVisible();
  const bounds = await icon.boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds!.width).toBeCloseTo(14, 1);
  expect(bounds!.height).toBeCloseTo(14, 1);
});
