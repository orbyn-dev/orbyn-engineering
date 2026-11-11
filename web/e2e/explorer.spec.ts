import { expect, test, type Page } from "@playwright/test";

/** The left explorer, against the mock service: the same three reads as the real one. */
const tree = (page: Page) => page.getByRole("tree", { name: "Projects, worktrees and files" });
const row = (page: Page, name: string | RegExp) => tree(page).getByRole("treeitem", { name }).first();

test.beforeEach(async ({ page }) => {
  await page.goto("/__mock/scenario/default");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/#/threads/t-done");
  await page.getByRole("tab", { name: "Files" }).click();
  await expect(tree(page)).toBeVisible();
});

test("the Files view lists the thread's project and unfolds its own worktree, and the Files tab is gone from the panel", async ({ page }) => {
  await expect(row(page, /^main/)).toHaveAttribute("aria-expanded", "true");
  await expect(row(page, /^README\.md/)).toBeVisible();
  await expect(row(page, /^approval-timeout/)).toHaveAttribute("aria-expanded", "false");
  await page.getByRole("button", { name: "Show side panel" }).click();
  await page.getByRole("button", { name: "Open a tab" }).click();
  await expect(page.getByRole("menuitem", { name: "Files" })).toHaveCount(0);
});

test("a folder is read when it is unfolded, and a file opens as an icon-only tab named by its file", async ({ page }) => {
  const reads: string[] = [];
  page.on("request", (request) => {
    if (request.url().includes("/worktrees/")) reads.push(request.url());
  });
  await row(page, /^src/).click();
  await expect(row(page, /^main\.rs/)).toBeVisible();
  expect(reads.filter((url) => url.includes("path=src"))).toHaveLength(1);
  await row(page, /^main\.rs/).click();
  const open = page.getByRole("tab", { name: "main.rs" });
  await expect(open).toHaveAttribute("aria-selected", "true");
  await expect(open).toHaveText("");
  await expect(page.locator(".ctx-title b")).toHaveText("main.rs");
  await expect(row(page, /^main\.rs/)).toHaveAttribute("aria-selected", "true");
  // The tab closes like any other, and a closed file is gone.
  await page.getByRole("button", { name: "Open a tab" }).click();
  await page.getByRole("menuitem", { name: "Tools" }).click();
  await open.click();
  await page.getByRole("button", { name: "Tab menu" }).click();
  await page.getByRole("menuitem", { name: "Close tab" }).click();
  await expect(page.getByRole("tab", { name: "main.rs" })).toHaveCount(0);
});

test("another worktree is read-only: its row says so and its file opens with a banner and a way to start a thread there", async ({ page }) => {
  await row(page, /^approval-timeout/).click();
  await expect(row(page, /^approval-timeout/).getByText("Read-only")).toBeVisible();
  await tree(page).getByRole("treeitem", { name: /^README\.md/ }).last().click();
  await expect(page.getByText("Read-only · not this thread’s worktree")).toBeVisible();
  await expect(page.getByText("// approval-timeout")).toBeVisible();
  await page.getByRole("button", { name: "Start a thread here" }).click();
  await expect(page).toHaveURL(/#\/new\/p-app$/);
});

test("a worktree whose folder is gone is one error row with Try again, and a locked one still reads", async ({ page }) => {
  await row(page, /^feat\/gone/).click();
  const alert = tree(page).getByRole("treeitem", { name: "Couldn’t read this folder." });
  await expect(alert).toContainText("Couldn’t read this folder.");
  await expect(alert.getByRole("button", { name: "Try again" })).toBeVisible();
  await row(page, /^feat\/locked/).click();
  await expect(row(page, /^feat\/locked/).getByText("Read-only")).toBeVisible();
});

test("a large folder shows 200 children and a row for the rest", async ({ page }) => {
  await row(page, /^src/).click();
  await row(page, /^generated/).click();
  // Only the rows on screen are in the page, whatever the folder holds.
  await expect(row(page, /^file-000/)).toBeVisible();
  expect(await tree(page).getByRole("treeitem").count()).toBeLessThan(80);
  const scroll = () => tree(page).evaluate((element) => { element.parentElement!.scrollTop = element.parentElement!.scrollHeight; });
  await scroll();
  await expect(tree(page).getByText("Show 60 more")).toBeVisible();
  await tree(page).getByText("Show 60 more").click();
  await scroll();
  await expect(row(page, /^file-259/)).toBeVisible();
  await expect(tree(page).getByText(/Show \d+ more/)).toHaveCount(0);
});

test("the arrow keys walk the tree, and the last view is remembered across a reload", async ({ page }) => {
  await row(page, /^docs/).focus();
  await page.keyboard.press("ArrowRight");
  await expect(row(page, /^docs/)).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("ArrowRight");
  await expect(row(page, /^guide\.md/)).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("tab", { name: "guide.md", selected: true })).toBeFocused();
  await page.reload();
  await expect(tree(page)).toBeVisible();
  await expect(page.getByRole("tab", { name: "Files" })).toHaveAttribute("aria-selected", "true");
});

test("at the minimum width the switch is two icons and file names shorten in the middle", async ({ page }) => {
  await page.evaluate(() => {
    const prefs = JSON.parse(localStorage.getItem("orbyn.prefs.v1") ?? "{}");
    localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ ...prefs, sidebarWidth: 180 }));
  });
  await page.reload();
  await expect(tree(page)).toBeVisible();
  await expect(page.getByRole("tab", { name: "Files" })).toHaveText("");
  await row(page, /^src/).click();
  await row(page, /^auth/).click();
  const long = row(page, /^redirect\.test\.ts/);
  await expect(long).toBeVisible();
  await expect(long).toHaveAttribute("title", "redirect.test.ts");
});
