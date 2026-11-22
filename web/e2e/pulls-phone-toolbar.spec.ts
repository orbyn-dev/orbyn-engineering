import { expect, test } from "@playwright/test";

for (const width of [320, 375]) test(`pull request toolbar stays within the ${width}px phone viewport`, async ({ page }) => {
  await page.setViewportSize({ width, height: 640 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/pulls");
  await expect(page.getByRole("heading", { name: "Pull requests" })).toBeVisible();
  const main = page.locator("#main");
  await expect(main).toBeVisible();
  const sizes = await main.evaluate((element) => ({ scroll: element.scrollWidth, client: element.clientWidth }));
  expect(sizes.scroll).toBeLessThanOrEqual(sizes.client);
  const refresh = page.getByRole("button", { name: "Refresh", exact: true });
  await expect(refresh).toBeVisible();
  const box = await refresh.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.x + box!.width).toBeLessThanOrEqual(width);
  const selectors = page.locator(".pulls-sels");
  await selectors.evaluate((element) => { element.scrollLeft = element.scrollWidth; });
  const sort = page.getByRole("button", { name: /^Sort:/ });
  const rowBox = await selectors.boundingBox();
  const sortBox = await sort.boundingBox();
  expect(rowBox).not.toBeNull();
  expect(sortBox).not.toBeNull();
  expect(sortBox!.x).toBeGreaterThanOrEqual(rowBox!.x);
  expect(sortBox!.x + sortBox!.width).toBeLessThanOrEqual(rowBox!.x + rowBox!.width);
  await sort.click();
  await expect(sort).toHaveAttribute("aria-expanded", "true");
});
