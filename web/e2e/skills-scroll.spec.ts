import { expect, test } from "@playwright/test";

test("Back restores the skills list position and focus after opening a card", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 768 });
  await page.route("**/api/skills", async (route) => {
    const response = await route.fetch();
    const catalog = await response.json();
    const examples = catalog.skills;
    catalog.skills = Array.from({ length: 45 }, (_, index) => ({
      ...examples[index % examples.length],
      name: `skill-${String(index).padStart(2, "0")}`,
    }));
    // The list can arrive after the route has already been displayed.
    await new Promise((resolve) => setTimeout(resolve, 300));
    await route.fulfill({ response, json: catalog });
  });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/skills");
  const content = page.locator(".skills-content");
  const card = page.locator('[data-skill="skill-21"]');
  await expect(card).toBeAttached();
  await content.evaluate((element) => { element.scrollTop = 700; });
  await expect.poll(() => content.evaluate((element) => element.scrollTop)).toBe(700);
  await card.click();
  await expect(page.getByRole("heading", { name: "skill-21" })).toBeFocused();
  await page.goBack();
  await expect(card).toBeFocused();
  await expect.poll(() => content.evaluate((element) => element.scrollTop)).toBe(700);
  await page.goForward();
  await expect(page.getByRole("heading", { name: "skill-21" })).toBeFocused();
  await page.goBack();
  await expect(card).toBeFocused();
  await expect.poll(() => content.evaluate((element) => element.scrollTop)).toBe(700);
});
