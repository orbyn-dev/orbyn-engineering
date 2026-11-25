import { expect, test, type Page } from "@playwright/test";
import type { Project, Thread } from "../src/api/types";

/**
 * Characterization of the sidebar's keyboard behaviour: the tree keys (treeKeys.ts), the thread search field
 * (ThreadSearchField.tsx), the selection dismissal and the jump-number hints (sidebarHooks.ts).
 * The default mock scenario draws, top to bottom: the Working group head, its one running thread, the
 * orbyn-engineering-app folder with its threads, then the marketing-site folder.
 *
 * Rows are named by title, and the counts these tests assert are derived from the scenario's own data:
 * the drawn DOM order for the jump badges, and the mock's thread list for the search results. A fixture
 * that gains a thread or reorders one changes the expectation with it.
 */

const tree = (page: Page) => page.locator(".sidebar-scroll");
/** What the arrow keys walk: thread rows and group heads, in drawn order. Folder headers are not among them. */
const rows = (page: Page) => tree(page).locator("a.thread-row, button.group-head");
const row = (page: Page, title: string) => tree(page).locator("a.thread-row", { hasText: title });
/** The thread rows the tree draws, in drawn order: jump badges number these, never a folder header. */
const threadRows = (page: Page) => tree(page).locator("li[data-thread-item]");
/** How many of those rows can show a badge: the first nine drawn rows, or every row when there are fewer. */
const badgeCount = async (page: Page) => Math.min(9, await threadRows(page).count());

/** The text a thread row is searched by (sidebarHooks.ts `searchFields`). */
const searchFields = (thread: Thread, project: string) =>
  `${thread.title} ${project} ${thread.worktree?.branch ?? ""}`.toLowerCase();

/**
 * The rows the thread search must list for a query, from the mock's own threads: neither archived
 * nor a side chat, the query in their searchable text, newest first. Both the set and the order are
 * derived here, so a fixture that gains or reorders a thread cannot break a fixed count or position.
 */
const expectedHits = async (page: Page, query: string) => {
  const threads = (await (await page.request.get("/api/threads")).json()) as Thread[];
  const projects = (await (await page.request.get("/api/projects")).json()) as Project[];
  const needle = query.trim().toLowerCase();
  return threads
    .filter((thread) => !thread.lifecycle?.archived_at && thread.lifecycle?.relation !== "side")
    .filter((thread) => searchFields(thread, projects.find((project) => project.id === thread.project_id)?.name ?? "").includes(needle))
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at))
    .map((thread) => ({ id: thread.id, title: thread.title }));
};

test.beforeEach(async ({ page }) => {
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/threads/t-done");
  await expect(row(page, "Explain how the event log resumes")).toBeVisible();
});

test.describe("tree keys", () => {
  test("ArrowDown and ArrowUp move focus between rows and stop at both ends", async ({ page }) => {
    const second = row(page, "Add approval timeout handling");
    await second.focus();
    // The next stop is whatever is drawn right after it; the default scenario's thread order is not pinned here.
    const all = rows(page);
    const secondAt = (await all.evaluateAll((nodes) => nodes.map((node) => node === document.activeElement))).indexOf(true);
    expect(secondAt).toBeGreaterThanOrEqual(0);
    expect(await all.count()).toBeGreaterThan(secondAt + 1);
    const third = all.nth(secondAt + 1);
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
    const header = tree(page).locator("li.folder-head").first();
    await expect(header).toBeVisible();
    // The stops around the header are the row drawn just before it and the folder's first thread; the header between them is not one.
    const before = header.locator("xpath=preceding::li[@data-thread-item][1]").locator("a.thread-row");
    const after = header.locator("xpath=following::li[@data-thread-item][1]").locator("a.thread-row");
    await expect(before).toHaveCount(1);
    await expect(after).toHaveCount(1);
    await before.focus();
    await page.keyboard.press("ArrowDown");
    await expect(after).toBeFocused();
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
  const highlighted = (page: Page) =>
    hits(page).evaluateAll((nodes) => nodes.map((node) => node.getAttribute("data-highlight") === "true"));
  const highlightedAt = (page: Page) =>
    hits(page).evaluateAll((nodes) => nodes.findIndex((node) => node.getAttribute("data-highlight") === "true"));
  /** Opens the field, types a query, and checks the listed rows against the mock's own threads. */
  const search = async (page: Page, query: string) => {
    const expected = await expectedHits(page, query);
    await open(page);
    await field(page).fill(query);
    await expect(hits(page)).toHaveCount(expected.length);
    expect(await hits(page).evaluateAll((nodes) => nodes.map((node) => node.getAttribute("data-thread-item"))))
      .toEqual(expected.map((hit) => hit.id));
    expect(await hits(page).locator(".thread-row-title").allTextContents())
      .toEqual(expected.map((hit) => hit.title));
    return expected;
  };

  test("typing lists the matching rows and highlights the first", async ({ page }) => {
    const expected = await search(page, "tool call");
    expect(expected.length).toBeGreaterThan(1);
    expect(await highlighted(page)).toEqual(expected.map((_, at) => at === 0));
  });

  test("ArrowDown and ArrowUp move the highlight and stop at the ends", async ({ page }) => {
    const expected = await search(page, "tool call");
    expect(expected.length).toBeGreaterThan(1);
    // Up from the first match stays on it; each Down moves one match; the last match holds.
    await expect.poll(() => highlightedAt(page)).toBe(0);
    await page.keyboard.press("ArrowUp");
    await expect.poll(() => highlightedAt(page)).toBe(0);
    for (let at = 1; at < expected.length; at++) {
      await page.keyboard.press("ArrowDown");
      await expect.poll(() => highlightedAt(page)).toBe(at);
    }
    await page.keyboard.press("ArrowDown");
    await expect.poll(() => highlightedAt(page)).toBe(expected.length - 1);
    for (let at = expected.length - 2; at >= 0; at--) {
      await page.keyboard.press("ArrowUp");
      await expect.poll(() => highlightedAt(page)).toBe(at);
    }
    // Focus never leaves the field while the highlight moves.
    await expect(field(page)).toBeFocused();
  });

  test("Enter opens the highlighted result", async ({ page }) => {
    const expected = await search(page, "tool call");
    expect(expected.length).toBeGreaterThan(1);
    const second = hits(page).nth(1);
    const id = await second.getAttribute("data-thread-item");
    await page.keyboard.press("ArrowDown");
    await expect(second).toHaveAttribute("data-highlight", "true");
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(new RegExp(`#/threads/${id}$`));
  });

  test("Enter with no match goes nowhere", async ({ page }) => {
    const before = page.url();
    await open(page);
    await field(page).fill("zzz-no-such-thread");
    await expect(page.getByText("No matching threads")).toBeVisible();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(before);
  });

  test("Escape clears the query and closes the field", async ({ page }) => {
    await search(page, "tool call");
    await page.keyboard.press("Escape");
    await expect(field(page)).toHaveCount(0);
    await expect(hits(page)).toHaveCount(0);
    // The full list is back, and reopening starts from an empty query.
    await expect(row(page, "Explain how the event log resumes")).toBeVisible();
    await open(page);
    await expect(field(page)).toHaveValue("");
  });

  test("the toolbar button closes the field and clears the query too", async ({ page }) => {
    await search(page, "tool call");
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
    await page.locator(".side-label", { hasText: /^Threads/ }).click({ position: { x: 4, y: 4 } });
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
  /** The badge numbers of the drawn thread rows, in drawn order; "" for a row without one. */
  const badges = (page: Page) =>
    threadRows(page).evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.jumpHint ?? ""));

  test("show on the first drawn rows while Mod is held, and go when it is released", async ({ page }) => {
    // The scenario draws more rows than the nine-badge cap, so both the leading set and the cap are exercised.
    const total = await threadRows(page).count();
    expect(total).toBeGreaterThan(9);
    const shown = Math.min(9, total);
    await expect(hinted(page)).toHaveCount(0);
    await page.keyboard.down("ControlOrMeta");
    // Nine at most, numbered in drawn order: badge k sits on the k-th drawn row, and no later row gets one.
    await expect(hinted(page)).toHaveCount(shown);
    expect(await badges(page)).toEqual(
      [...Array(shown)].map((_, at) => String(at + 1)).concat(Array(total - shown).fill("")),
    );
    // The Working shelf is drawn above the project folders, so its running row leads.
    await expect(threadRows(page).first()).toHaveAttribute("data-section", "working");
    await expect(threadRows(page).first()).toHaveAttribute("data-jump-hint", "1");
    await page.keyboard.up("ControlOrMeta");
    await expect(hinted(page)).toHaveCount(0);
  });

  test("Mod with Shift or Alt shows none", async ({ page }) => {
    const shown = await badgeCount(page);
    expect(shown).toBeGreaterThan(0);
    await page.keyboard.down("ControlOrMeta");
    await expect(hinted(page)).toHaveCount(shown);
    await page.keyboard.down("Shift");
    await expect(hinted(page)).toHaveCount(0);
    await page.keyboard.up("Shift");
    await expect(hinted(page)).toHaveCount(shown);
    await page.keyboard.down("Alt");
    await expect(hinted(page)).toHaveCount(0);
    await page.keyboard.up("Alt");
    await page.keyboard.up("ControlOrMeta");
  });

  test("losing window focus clears them", async ({ page }) => {
    const shown = await badgeCount(page);
    expect(shown).toBeGreaterThan(0);
    await page.keyboard.down("ControlOrMeta");
    await expect(hinted(page)).toHaveCount(shown);
    await page.evaluate(() => window.dispatchEvent(new Event("blur")));
    await expect(hinted(page)).toHaveCount(0);
    await page.keyboard.up("ControlOrMeta");
  });
});
