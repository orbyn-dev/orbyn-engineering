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

for (const state of ["loading", "error"] as const) {
  for (const detail of [false, true]) {
    test(`phone can leave ${state} Skills ${detail ? "detail" : "list"}`, async ({ page }) => {
      await page.setViewportSize({ width: 375, height: 800 });
      await page.goto("/__mock/scenario/default", { waitUntil: "domcontentloaded" });
      let release!: () => void;
      let requested = false;
      const held = new Promise<void>((resolve) => { release = resolve; });
      await page.route("**/api/skills", async (route) => {
        requested = true;
        if (state === "loading") await held;
        await route.fulfill({ status: 503, contentType: "application/json", body: JSON.stringify({ error: { code: "unavailable", message: "Unavailable" } }) });
      });
      await page.goto(detail ? "/#/skills/architect" : "/#/skills", { waitUntil: "domcontentloaded" });
      await expect.poll(() => requested).toBe(true);
      if (state === "error") await expect(page.getByText("Couldn't load skills")).toBeVisible();
      if (detail) {
        await page.getByRole("link", { name: "Back to skills" }).click();
        await expect(page).toHaveURL(/#\/skills$/);
      }
      await page.getByRole("link", { name: "Back to threads" }).click();
      await expect(page).toHaveURL(/#\/$/);
      release();
    });
  }
}
