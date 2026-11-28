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

/** shell-ledger: thread titles win over the project chip on a narrow row, and a row stays one line. */
test("a narrow sidebar row hides the project chip and keeps one line", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ workspaceLayout: "ledger", sidebarWidth: 180 })));
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/threads/t-done");
  const rows = page.locator(".thread-row");
  await expect(rows.first()).toBeVisible();
  const narrow = await rows.first().evaluate((row) => row.getBoundingClientRect().width);
  expect(narrow).toBeLessThanOrEqual(240);
  await expect(page.locator(".thread-row-project:visible")).toHaveCount(0);
  const heights = await rows.evaluateAll((all) => all.map((row) => Math.round(row.getBoundingClientRect().height)));
  expect(Math.max(...heights)).toBeLessThanOrEqual(36);
});

/** A hovered project row's plus never covers the project name. */
test("the hovered plus on a project row does not overlap the project name", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/threads/t-done");
  const head = page.locator(".folder-head").first();
  await expect(head).toBeVisible();
  await head.hover();
  const plus = await head.locator(".folder-new").boundingBox();
  const name = await head.locator(".folder-name").evaluate((node) => {
    const box = node.getBoundingClientRect();
    return { right: box.right - parseFloat(getComputedStyle(node).paddingRight) };
  });
  expect(plus).not.toBeNull();
  expect(name.right).toBeLessThanOrEqual((plus?.x ?? 0) + 0.5);
});

/**
 * A project row's waiting count is never covered by the plus that floats over the row's right end: the row reserves the
 * plus's footprint, so one digit and three stop at the same place, and the count stays part of the toggle it sits in.
 */
for (const [name, prefs] of [["default sidebar", {}], ["narrow sidebar", { sidebarWidth: 180 }]] as const) {
  test(`the hovered plus on a project row clears the waiting count (${name})`, async ({ page }) => {
    await page.addInitScript((saved) => localStorage.setItem("orbyn.prefs.v1", JSON.stringify(saved)), prefs);
    await page.setViewportSize({ width: 1280, height: 820 });
    await page.goto("/__mock/scenario/default");
    await page.goto("/#/threads/t-done");
    const head = page.locator(".folder-head").filter({ has: page.locator(".folder-needs") }).first();
    await expect(head).toBeVisible();
    const toggle = head.locator(".folder-toggle");
    await head.hover();
    // Hovered, the plus is painted: this is the state the count was found covered in.
    await expect(head.locator(".folder-new")).toHaveCSS("opacity", "1");
    // The count grows leftwards from the row's right end, so the plus keeps its gap whatever the count's width.
    for (const countText of ["1", "128"]) {
      const measured = await head.evaluate((row, text) => {
        const count = row.querySelector(".folder-needs") as HTMLElement;
        (count.querySelector("span[aria-hidden]") as HTMLElement).textContent = text;
        const plus = row.querySelector(".folder-new")!.getBoundingClientRect();
        const box = count.getBoundingClientRect();
        const line = row.getBoundingClientRect();
        return { count: { left: box.left, right: box.right, width: box.width }, plus: { left: plus.left, right: plus.right }, row: { left: line.left, right: line.right } };
      }, countText);
      expect(measured.count.width, `a ${countText} count is drawn`).toBeGreaterThan(0);
      expect(measured.count.right, `the ${countText} count stops short of the plus`).toBeLessThanOrEqual(measured.plus.left);
      expect(measured.count.left, "the count stays inside the row").toBeGreaterThanOrEqual(measured.row.left - 0.5);
      expect(measured.plus.right, "the plus stays inside the row").toBeLessThanOrEqual(measured.row.right + 0.5);
    }
    // The count belongs to the toggle: clicking it folds the project rather than starting a thread in it.
    const count = await head.locator(".folder-needs").boundingBox();
    const expanded = await toggle.getAttribute("aria-expanded");
    expect(count).not.toBeNull();
    await page.mouse.click((count?.x ?? 0) + (count?.width ?? 0) / 2, (count?.y ?? 0) + (count?.height ?? 0) / 2);
    await expect(toggle).toHaveAttribute("aria-expanded", expanded === "true" ? "false" : "true");
    await expect(page).toHaveURL(/#\/threads\/t-done$/);
    // Folded, the row shows its total in the same place: it stops short of the plus too.
    const totals = await head.evaluate((row) => {
      const plus = row.querySelector(".folder-new")!.getBoundingClientRect();
      const total = row.querySelector(".group-count")!.getBoundingClientRect();
      return { plusLeft: plus.left, totalRight: total.right };
    });
    expect(totals.totalRight).toBeLessThanOrEqual(totals.plusLeft);
  });
}

/**
 * Row actions show only on pointer hover or keyboard focus, the open row included, and always for multi-selected rows.
 * Hidden, they are out of the Tab order and cover nothing: the open row's status text stays visible.
 */
test("row actions show on hover and focus only, and leave the Tab order and the status text alone otherwise", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/threads/t-done");
  await page.mouse.move(700, 400);
  const open = page.locator(".thread-row[aria-current=page]");
  await expect(open).toBeVisible();
  // The open row: no strip without hover or focus, and its trailing text is not covered.
  const strip = open.locator(".row-actions");
  await expect(open.getByRole("button", { name: "Settle thread" })).toBeHidden();
  // The status text is there and not covered (the strip is hidden, so nothing paints over it).
  await expect(strip).toBeHidden();
  const rowsWithStatus = page.locator(".thread-row").filter({ has: page.locator(".row-rest", { hasText: /\S/ }) });
  expect(await rowsWithStatus.count()).toBeGreaterThan(0);
  await expect(rowsWithStatus.first().locator(".row-rest")).toBeVisible();
  // Every row's buttons are hidden, so Tab cannot land on them.
  const all = page.locator(".thread-row .row-action");
  const count = await all.count();
  expect(count).toBeGreaterThan(3);
  for (let at = 0; at < Math.min(count, 12); at += 1) await expect(all.nth(at)).toBeHidden();
  // Pointer hover shows them, with the tooltips the frames name.
  await open.hover();
  await expect(open.getByRole("button", { name: "Settle thread" })).toBeVisible();
  await expect(open.getByRole("button", { name: "Settle thread" })).toHaveAttribute("title", "Settle");
  await expect(open.getByRole("button", { name: "Snooze thread" })).toHaveAttribute("title", "Snooze");
  await expect(strip).toBeVisible();
  await page.mouse.move(700, 400);
  await expect(open.getByRole("button", { name: "Settle thread" })).toBeHidden();
  // Keyboard focus on another row shows its own strip.
  const other = page.locator(".thread-row:not([aria-current=page])").nth(2);
  await other.focus();
  await expect(other.locator(".row-action").first()).toBeVisible();
  // A multi-selected row shows them with no hover and no focus on it.
  await other.click({ modifiers: ["ControlOrMeta"] });
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.mouse.move(700, 400);
  const selected = page.locator("li[data-selected] .thread-row");
  await expect(selected.first()).toBeVisible();
  await expect(selected.first().locator(".row-action").first()).toBeVisible();
});

/** A mouse click focuses the row without :focus-visible, so the strip must not stay once the pointer has left. */
test("after a click the strip goes away with the pointer and the row's status text stays uncovered", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/threads/t-done");
  // The open row: clicking it does not navigate away, so the click leaves it the focused element.
  const row = page.locator(".thread-row[aria-current=page]");
  await expect(row).toBeVisible();
  await row.click();
  await expect.poll(() => page.evaluate(() => document.activeElement?.classList.contains("thread-row"))).toBe(true);
  await page.mouse.move(700, 400);
  await expect(row.locator(".row-action").first()).toBeHidden();
  // A row with a status or age shows it, uncovered, with its neighbour's pointer long gone.
  const withStatus = page.locator(".thread-row").filter({ has: page.locator(".row-rest", { hasText: /\S/ }) }).first();
  await expect(withStatus.locator(".row-rest")).toBeVisible();
  // Keyboard focus is another matter: Tab shows the strip, and Tab again goes into its buttons without hiding it.
  await row.focus();
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Tab");
  await expect(row.locator(".row-action").first()).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(row.locator(".row-action").first()).toBeVisible();
});
