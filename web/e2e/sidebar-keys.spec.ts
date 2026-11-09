import { expect, test, type Page } from "@playwright/test";

/**
 * Characterization of the sidebar's keyboard behaviour: the tree keys (treeKeys.ts), the thread search field
 * (ThreadSearchField.tsx), the selection dismissal and the jump-number hints (sidebarHooks.ts).
 * The default mock scenario draws, top to bottom: the Working group head, its one running thread, the
 * orbyn-engineering-app folder with its threads, then the marketing-site folder.
 */

const tree = (page: Page) => page.locator(".sidebar-scroll");
/** What the arrow keys walk: thread rows and group heads, in drawn order. Folder headers are not among them. */
const rows = (page: Page) => tree(page).locator("a.thread-row, button.group-head");
const row = (page: Page, title: string) => tree(page).locator("a.thread-row", { hasText: title });

test.beforeEach(async ({ page }) => {
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/threads/t-done");
  await expect(row(page, "Explain how the event log resumes")).toBeVisible();
});

test.describe("tree keys", () => {
  test("ArrowDown and ArrowUp move focus between rows and stop at both ends", async ({ page }) => {
    const second = row(page, "Add approval timeout handling");
    const third = row(page, "Tool call states");
    await second.focus();
    await page.keyboard.press("ArrowDown");
    await expect(third).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await expect(second).toBeFocused();

    // The Working group head is the first stop: ArrowUp there goes nowhere.
    const first = rows(page).first();
    await expect(first).toHaveClass(/group-head/);
    await first.focus();
    await page.keyboard.press("ArrowUp");
    await expect(first).toBeFocused();

    // The last stop is the project that has no threads yet.
    const last = rows(page).last();
    await last.focus();
    await page.keyboard.press("ArrowDown");
    await expect(last).toBeFocused();
  });

  // Current behaviour, not necessarily desired: folder headers are not arrow-key stops.
  test("the arrows step over a project folder's header", async ({ page }) => {
    const running = row(page, "Tool calls, running");
    await running.focus();
    await page.keyboard.press("ArrowDown");
    // The orbyn-engineering-app folder header sits between the two, and is not a stop.
    await expect(row(page, "Add approval timeout handling")).toBeFocused();
  });

  test("ArrowLeft collapses and ArrowRight expands a group head", async ({ page }) => {
    const head = tree(page).locator("button.group-head", { hasText: "Working" });
    await head.focus();
    await expect(head).toHaveAttribute("aria-expanded", "true");
    await page.keyboard.press("ArrowRight");
    await expect(head).toHaveAttribute("aria-expanded", "true");
    await page.keyboard.press("ArrowLeft");
    await expect(head).toHaveAttribute("aria-expanded", "false");
    await page.keyboard.press("ArrowLeft");
    await expect(head).toHaveAttribute("aria-expanded", "false");
    await page.keyboard.press("ArrowRight");
    await expect(head).toHaveAttribute("aria-expanded", "true");
    await expect(head).toBeFocused();
  });

  // Current behaviour, not necessarily desired: only group heads answer ArrowLeft and ArrowRight.
  test("ArrowLeft and ArrowRight on a thread row do nothing", async ({ page }) => {
    const done = row(page, "Explain how the event log resumes");
    await done.focus();
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("ArrowRight");
    await expect(done).toBeFocused();
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(page.getByRole("menu")).toHaveCount(0);
  });

  test("F2 starts renaming the focused thread", async ({ page }) => {
    await row(page, "Tool call states").focus();
    await page.keyboard.press("F2");
    const dialog = page.getByRole("dialog", { name: "Rename thread" });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("textbox", { name: "Thread title" })).toHaveValue("Tool call states");
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
  });

  test("F2 on a group head renames nothing", async ({ page }) => {
    await tree(page).locator("button.group-head", { hasText: "Working" }).focus();
    await page.keyboard.press("F2");
    await expect(page.getByRole("dialog")).toHaveCount(0);
  });

  for (const [name, press] of [
    ["Shift+F10", "Shift+F10"],
    ["the ContextMenu key", "ContextMenu"],
  ] as const) {
    test(`${name} opens the focused row's menu below it`, async ({ page }) => {
      // Tall enough that the menu is never pushed up to fit the window.
      await page.setViewportSize({ width: 1280, height: 1600 });
      const target = row(page, "Tool call states");
      await target.focus();
      await page.keyboard.press(press);
      const menu = page.getByRole("menu", { name: "Actions for Tool call states" });
      await expect(menu).toBeVisible();
      await expect(menu.getByRole("menuitem", { name: "Rename thread" })).toBeVisible();
      // The menu opens at the row's left edge plus 12px, just under it. Its placed position is read from
      // its inline left/top, not its bounding box: the box is scaled while the open animation runs, so a
      // box read is off by a few pixels and flaky.
      const at = await target.boundingBox();
      const placed = await menu.evaluate((node) => ({ x: parseFloat((node as HTMLElement).style.left), y: parseFloat((node as HTMLElement).style.top) }));
      if (!at) throw new Error("no box");
      expect(placed.x).toBeCloseTo(at.x + 12, 0);
      expect(placed.y).toBeCloseTo(at.y + at.height, 0);
      await page.keyboard.press("Escape");
      await expect(menu).toBeHidden();
    });
  }

  test("a plain F10 opens no menu", async ({ page }) => {
    await row(page, "Tool call states").focus();
    await page.keyboard.press("F10");
    await expect(page.getByRole("menu")).toHaveCount(0);
  });
});

test.describe("thread search field", () => {
  const field = (page: Page) => page.getByRole("searchbox", { name: "Search threads" });
  const open = async (page: Page) => {
    await page.getByRole("button", { name: "Search threads" }).click();
    await expect(field(page)).toBeFocused();
  };
  const hits = (page: Page) => page.getByRole("list", { name: "Thread search results" }).locator("[data-thread-item]");

  test("typing lists matches and highlights the first", async ({ page }) => {
    await open(page);
    await field(page).fill("tool call");
    await expect(hits(page)).toHaveCount(2);
    await expect(hits(page).nth(0)).toHaveAttribute("data-highlight", "true");
    await expect(hits(page).nth(1)).not.toHaveAttribute("data-highlight", "true");
  });

  test("ArrowDown and ArrowUp move the highlight and stop at the ends", async ({ page }) => {
    await open(page);
    await field(page).fill("tool call");
    await expect(hits(page)).toHaveCount(2);
    await page.keyboard.press("ArrowUp");
    await expect(hits(page).nth(0)).toHaveAttribute("data-highlight", "true");
    await page.keyboard.press("ArrowDown");
    await expect(hits(page).nth(1)).toHaveAttribute("data-highlight", "true");
    await expect(hits(page).nth(0)).not.toHaveAttribute("data-highlight", "true");
    await page.keyboard.press("ArrowDown");
    await expect(hits(page).nth(1)).toHaveAttribute("data-highlight", "true");
    await page.keyboard.press("ArrowUp");
    await expect(hits(page).nth(0)).toHaveAttribute("data-highlight", "true");
    // Focus never leaves the field while the highlight moves.
    await expect(field(page)).toBeFocused();
  });

  test("Enter opens the highlighted result", async ({ page }) => {
    await open(page);
    await field(page).fill("tool call");
    await expect(hits(page)).toHaveCount(2);
    const second = hits(page).nth(1);
    const id = await second.getAttribute("data-thread-item");
    await page.keyboard.press("ArrowDown");
    await expect(second).toHaveAttribute("data-highlight", "true");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(new RegExp(`#/threads/${id}$`));
  });

  test("Enter with no match goes nowhere", async ({ page }) => {
    await open(page);
    await field(page).fill("zzz-no-such-thread");
    await expect(page.getByText("No matching threads")).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/#\/threads\/t-done$/);
  });

  test("Escape clears the query and closes the field", async ({ page }) => {
    await open(page);
    await field(page).fill("tool call");
    await expect(hits(page)).toHaveCount(2);
    await page.keyboard.press("Escape");
    await expect(field(page)).toHaveCount(0);
    await expect(hits(page)).toHaveCount(0);
    // The full list is back, and reopening starts from an empty query.
    await expect(row(page, "Explain how the event log resumes")).toBeVisible();
    await open(page);
    await expect(field(page)).toHaveValue("");
  });

  test("the toolbar button closes the field and clears the query too", async ({ page }) => {
    await open(page);
    await field(page).fill("tool call");
    await expect(hits(page)).toHaveCount(2);
    await page.getByRole("button", { name: "Close thread search" }).click();
    await expect(field(page)).toHaveCount(0);
    await expect(hits(page)).toHaveCount(0);
  });
});

test.describe("selection dismissal", () => {
  const picked = (page: Page) => tree(page).locator("li[data-selected]");
  const select = async (page: Page, ...titles: string[]) => {
    for (const title of titles) await row(page, title).click({ modifiers: ["ControlOrMeta"] });
    await expect(picked(page)).toHaveCount(titles.length);
  };

  test("Mod-click builds a multi-selection", async ({ page }) => {
    await select(page, "Tool call states", "Explain how the event log resumes");
    // The route did not change: Mod-click selects, it does not open.
    await expect(page).toHaveURL(/#\/threads\/t-done$/);
  });

  test("a click outside the rows clears it", async ({ page }) => {
    await select(page, "Tool call states", "Explain how the event log resumes");
    await page.locator(".msg-bubble").first().click();
    await expect(picked(page)).toHaveCount(0);
  });

  test("a click on the sidebar's own header clears it", async ({ page }) => {
    await select(page, "Tool call states", "Explain how the event log resumes");
    await page.locator(".side-label", { hasText: /^Projects/ }).click({ position: { x: 4, y: 4 } });
    await expect(picked(page)).toHaveCount(0);
  });

  test("Escape clears it", async ({ page }) => {
    await select(page, "Tool call states", "Explain how the event log resumes");
    await page.keyboard.press("Escape");
    await expect(picked(page)).toHaveCount(0);
  });

  test("a press on another row does not clear it", async ({ page }) => {
    await select(page, "Tool call states", "Explain how the event log resumes");
    await row(page, "Upgrade tokio").hover();
    await page.mouse.down();
    await expect(picked(page)).toHaveCount(2);
    await page.mouse.up();
  });

  test("a press inside an open menu does not clear it", async ({ page }) => {
    await select(page, "Tool call states", "Explain how the event log resumes");
    await row(page, "Tool call states").click({ button: "right" });
    const menu = page.getByRole("menu", { name: "Actions for 2 threads" });
    await expect(menu).toBeVisible();
    await menu.getByRole("menuitem", { name: /^Snooze/ }).hover();
    await page.mouse.down();
    await expect(picked(page)).toHaveCount(2);
    await page.mouse.up();
  });
});

test.describe("jump-number hints", () => {
  const hinted = (page: Page) => tree(page).locator("li[data-jump-hint]");

  test("show on the first rows while Mod is held, and go when it is released", async ({ page }) => {
    await expect(hinted(page)).toHaveCount(0);
    await page.keyboard.down("ControlOrMeta");
    // Nine at most, numbered in drawn order: the running thread leads, then the folder's threads.
    await expect(hinted(page)).toHaveCount(9);
    await expect(tree(page).locator("li[data-jump-hint='1']")).toContainText("Tool calls, running");
    await expect(tree(page).locator("li[data-jump-hint='2']")).toContainText("Add approval timeout handling");
    await expect(tree(page).locator("li[data-jump-hint='9']")).toHaveCount(1);
    await expect(tree(page).locator("li[data-jump-hint='10']")).toHaveCount(0);
    await page.keyboard.up("ControlOrMeta");
    await expect(hinted(page)).toHaveCount(0);
  });

  test("Mod with Shift or Alt shows none", async ({ page }) => {
    await page.keyboard.down("ControlOrMeta");
    await expect(hinted(page)).toHaveCount(9);
    await page.keyboard.down("Shift");
    await expect(hinted(page)).toHaveCount(0);
    await page.keyboard.up("Shift");
    await expect(hinted(page)).toHaveCount(9);
    await page.keyboard.down("Alt");
    await expect(hinted(page)).toHaveCount(0);
    await page.keyboard.up("Alt");
    await page.keyboard.up("ControlOrMeta");
  });

  test("losing window focus clears them", async ({ page }) => {
    await page.keyboard.down("ControlOrMeta");
    await expect(hinted(page)).toHaveCount(9);
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await expect(hinted(page)).toHaveCount(0);
    await page.keyboard.up("ControlOrMeta");
  });
});
