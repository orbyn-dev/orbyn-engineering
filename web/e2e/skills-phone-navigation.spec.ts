import { expect, test } from "@playwright/test";

for (const width of [320, 375]) {
  test(`Skills phone navigation at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    await page.goto("/__mock/scenario/default", { waitUntil: "domcontentloaded" });
    await page.goto("/#/skills", { waitUntil: "domcontentloaded" });
    const list = page.getByRole("region", { name: "Skills", exact: true });
    await expect(list).toBeVisible();
    const back = list.getByRole("link", { name: "Back to threads" });
    await expect(back).toBeVisible();
    const button = await back.boundingBox();
    const heading = await list.getByRole("heading", { name: "Skills", exact: true }).boundingBox();
    const grid = await list.locator(".skills-grid").boundingBox();
    expect(button?.width).toBe(44);
    expect(button?.height).toBe(44);
    expect(heading?.x).toBe(64);
    expect(grid?.x).toBe(16);
    await list.locator(".skills-card").first().click();
    const detail = page.getByRole("region", { name: "Skill", exact: true });
    const backToSkills = detail.getByRole("link", { name: "Back to skills" });
    await expect(backToSkills).toBeVisible();
    await backToSkills.click();
    await expect(list).toBeVisible();
    await back.click();
    await expect(page).toHaveURL(/#\/$/);
  });
}

test("Skills does not add phone back controls on desktop", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/__mock/scenario/default", { waitUntil: "domcontentloaded" });
  await page.goto("/#/skills", { waitUntil: "domcontentloaded" });
  const list = page.getByRole("region", { name: "Skills", exact: true });
  await expect(list).toBeVisible();
  await expect(list.getByRole("link", { name: "Back to threads" })).toHaveCount(0);
  await list.locator(".skills-card").first().click();
  const detail = page.getByRole("region", { name: "Skill", exact: true });
  await expect(detail).toBeVisible();
  await expect(detail.getByRole("link", { name: "Back to skills" })).toHaveCount(0);
});
