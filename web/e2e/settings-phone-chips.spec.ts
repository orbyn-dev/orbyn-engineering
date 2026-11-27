import { expect, test } from "@playwright/test";

for (const width of [320, 375, 390]) {
  test(`settings chips fit and remain keyboard accessible at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 768 });
    await page.goto("/__mock/scenario/default");
    await page.goto("/#/settings/general");
    const nav = page.getByRole("navigation", { name: "Settings sections" });
    const links = nav.getByRole("link");
    await expect(links.first()).toBeVisible();
    const sizes = await nav.evaluate((element) => {
      const first = element.querySelector("a")!.getBoundingClientRect();
      const navBounds = element.getBoundingClientRect();
      const side = element.closest<HTMLElement>(".settings-side")!;
      return {
        clientHeight: element.clientHeight,
        scrollHeight: element.scrollHeight,
        sideHeight: side.clientHeight,
        sideScrollHeight: side.scrollHeight,
        chipTop: first.top,
        chipBottom: first.bottom,
        navTop: navBounds.top,
        navBottom: navBounds.bottom,
        scrollbarWidth: getComputedStyle(element).scrollbarWidth,
      };
    });
    expect(sizes.scrollHeight).toBe(sizes.clientHeight);
    expect(sizes.sideScrollHeight).toBe(sizes.sideHeight);
    expect(sizes.chipTop).toBeGreaterThanOrEqual(sizes.navTop);
    expect(sizes.chipBottom).toBeLessThanOrEqual(sizes.navBottom);
    expect(sizes.scrollbarWidth).toBe("none");
    await links.first().focus();
    const count = await links.count();
    for (let index = 1; index < count; index++) await page.keyboard.press("Tab");
    const last = links.last();
    await expect(last).toBeFocused();
    const bounds = await nav.evaluate((element) => {
      const parent = element.getBoundingClientRect();
      const child = Array.from(element.querySelectorAll("a")).at(-1)?.getBoundingClientRect();
      return { left: parent.left, right: parent.right, lastLeft: child?.left, lastRight: child?.right, scrolled: element.scrollLeft };
    });
    expect(bounds.scrolled).toBeGreaterThan(0);
    expect(bounds.lastLeft).toBeGreaterThanOrEqual(bounds.left);
    expect(bounds.lastRight).toBeLessThanOrEqual(bounds.right);
  });
}

test("desktop settings navigation remains vertical", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 768 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/settings/general");
  const nav = page.getByRole("navigation", { name: "Settings sections" });
  await expect(nav.getByRole("link").first()).toBeVisible();
  expect(await nav.evaluate((element) => getComputedStyle(element).flexDirection)).toBe("column");
});
