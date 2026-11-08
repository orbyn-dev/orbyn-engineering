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

const centreOf = async (locator: import("@playwright/test").Locator) => {
  const box = await locator.boundingBox();
  return box ? box.y + box.height / 2 : NaN;
};

/** shell-console, and the same thread with the sidebar hidden and in a small window: one bar band, one title. */
for (const [name, width, hideSidebar] of [["wide", 1280, false], ["sidebar hidden", 1280, true], ["small window", 800, false]] as const) {
  test(`thread header actions share the bar's centre line, and the title appears once (${name})`, async ({ page }) => {
    await page.setViewportSize({ width, height: 820 });
    await page.goto("/__mock/scenario/default");
    await page.goto("/#/threads/t-done");
    if (hideSidebar) await page.getByRole("button", { name: "Hide sidebar" }).click();
    const heading = page.locator(".main h1").first();
    await expect(heading).toBeVisible();
    const bar = await centreOf(page.locator(".titlebar .tbi").first());
    for (const action of [page.getByRole("button", { name: "Details" }), page.getByRole("button", { name: "Terminal" }), page.getByRole("button", { name: /^Git/ })]) {
      if (await action.count()) expect(await centreOf(action.first())).toBeCloseTo(bar, 0);
    }
    expect(await centreOf(heading)).toBeCloseTo(bar, 0);
    // The bar never carries a title of its own: the page header is the one place (v60).
    await expect(page.locator(".titlebar-title")).toHaveCount(0);
    const text = (await heading.textContent())?.trim() ?? "";
    expect(await page.locator(".main, .titlebar").getByText(text, { exact: true }).count(), "one place for the title").toBe(1);
    // The header's first thing is right of the bar's buttons and never under them.
    const lead = await page.locator(".titlebar .tbi").last().boundingBox();
    const first = await heading.boundingBox();
    if (lead && first) expect(first.x).toBeGreaterThanOrEqual(lead.x + lead.width);
  });
}

/** shell-reading and thread-details-reading (v60): the same single bar band, with only the text column narrower. */
test("the reading layout has one 44px bar band and no second row or duplicate heading", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ workspaceLayout: "reading" })));
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/threads/t-done");
  const heading = page.locator(".main h1").first();
  await expect(heading).toBeVisible();
  const bar = await centreOf(page.locator(".titlebar .tbi").first());
  expect(bar).toBeCloseTo(22, 0);
  expect(await centreOf(heading)).toBeCloseTo(bar, 0);
  for (const name of ["Details", "Terminal"]) expect(await centreOf(page.getByRole("button", { name }).first())).toBeCloseTo(bar, 0);
  await expect(page.locator(".titlebar-title")).toHaveCount(0);
  const text = (await heading.textContent())?.trim() ?? "";
  expect(await page.locator(".main, .titlebar").getByText(text, { exact: true }).count(), "one place for the title").toBe(1);
  const header = await page.locator(".thread-main > .topbar").boundingBox();
  expect(header?.height).toBeLessThanOrEqual(46);
});

/** The bar's right end (environment pill, ledger's waiting count) never sits under the header's own actions. */
for (const layout of ["console", "ledger"] as const) {
  test(`the environment pill and the waiting count do not overlap the header actions (${layout})`, async ({ page }) => {
    await page.addInitScript((value) => localStorage.setItem("orbyn.prefs.v1", JSON.stringify(value)), { workspaceLayout: layout, environmentIdentification: "pill" });
    await page.setViewportSize({ width: 1280, height: 820 });
    await page.goto("/__mock/scenario/default");
    await page.goto("/#/threads/t-done");
    const pill = page.locator(".titlebar-end .environment-identity");
    await expect(pill).toBeVisible();
    if (layout === "ledger") await expect(page.locator(".titlebar-waiting")).toBeVisible();
    const end = await page.locator(".titlebar-end").boundingBox();
    expect(end).not.toBeNull();
    for (const action of [page.getByRole("button", { name: "Details" }), page.getByRole("button", { name: "Terminal" }), page.getByRole("button", { name: /^Git/ }), page.getByRole("button", { name: "More actions" })]) {
      if (!(await action.count())) continue;
      const box = await action.first().boundingBox();
      if (box && end) expect(box.x + box.width, "an action stops short of the bar's right end").toBeLessThanOrEqual(end.x);
    }
    expect(await centreOf(pill)).toBeCloseTo(22, 0);
  });
}

/** The settings overlay: "Find a setting" starts 12px under the 44px bar, with nothing but the bar in the bar's row. */
test("the settings search sits 12px below the title bar", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/settings/general");
  const search = page.getByRole("searchbox", { name: "Search settings" });
  await expect(search).toBeVisible();
  const field = await page.locator(".settings-search").boundingBox();
  expect(field?.y).toBeCloseTo(56, 0);
});
