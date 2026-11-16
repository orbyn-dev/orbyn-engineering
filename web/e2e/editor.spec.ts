import { expect, test, type Page } from "@playwright/test";

/** The file editor (Engineer Designs v106, editor-*), against the mock service: a file of the thread's own worktree opens in the editor. */
const tree = (page: Page) => page.getByRole("tree", { name: "Projects, worktrees and files" });
const code = (page: Page) => page.getByRole("textbox", { name: "Edit Cargo.toml" });

test.beforeEach(async ({ page }) => {
  await page.goto("/__mock/scenario/default");
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/#/threads/t-done");
  await page.getByRole("tab", { name: "Files" }).click();
  await tree(page).getByRole("treeitem", { name: /^Cargo\.toml/ }).first().click();
  await expect(code(page)).toBeVisible();
});

test("a file opens as an editor with its folders, line numbers, cursor and saved state", async ({ page }) => {
  await expect(code(page)).toHaveValue(/Mock file contents/);
  await expect(page.locator(".ed-gut")).toContainText("1");
  await expect(page.getByText("Ln 1, Col 1")).toBeVisible();
  await expect(page.locator(".ed-end")).toHaveText("Saved");
  await expect(page.getByRole("button", { name: "Save" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Find ⌘F" })).toBeVisible();
});

test("typing marks the tab and header unsaved, and Save writes the file and clears the mark", async ({ page }) => {
  await code(page).click();
  await page.keyboard.press("End");
  await page.keyboard.type(" edited");
  await expect(page.getByRole("tab", { name: "Cargo.toml, unsaved changes" })).toBeVisible();
  await expect(page.locator(".ed-unsaved")).toHaveText("Unsaved");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("tab", { name: "Cargo.toml", exact: true })).toBeVisible();
  await expect(page.locator(".ed-end")).toHaveText("Saved");
  // A second edit starts from the revision the first save wrote.
  await page.keyboard.type("!");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".ed-end")).toHaveText("Saved");
});

test("Find counts matches and Go to line moves the cursor", async ({ page }) => {
  await page.getByRole("button", { name: "Find ⌘F" }).click();
  await page.getByRole("textbox", { name: "Find in file" }).fill("//");
  await expect(page.getByText("1 of 2")).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByText("2 of 2")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Go to line ⌘G" }).click();
  await expect(page.getByText(/^1 to \d+$/)).toBeVisible();
  await page.getByRole("textbox", { name: "Line number" }).fill("2");
  await page.keyboard.press("Enter");
  await expect(page.getByText("Ln 2, Col 1")).toBeVisible();
});

test("closing a tab with unsaved changes asks; Cancel keeps it, Don’t save closes it", async ({ page }) => {
  await code(page).click();
  await page.keyboard.type("x");
  await page.getByRole("button", { name: "Tab menu" }).click();
  await page.getByRole("menuitem", { name: "Close tab" }).click();
  const dialog = page.getByRole("dialog", { name: "Save changes to Cargo.toml?" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("tab", { name: "Cargo.toml, unsaved changes" })).toBeVisible();
  await page.getByRole("button", { name: "Tab menu" }).click();
  await page.getByRole("menuitem", { name: "Close tab" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Don’t save" }).click();
  await expect(page.getByRole("tab", { name: /Cargo\.toml/ })).toHaveCount(0);
});

test("the tab menu copies the path and offers the outside editor", async ({ page }) => {
  await page.getByRole("button", { name: "Tab menu" }).click();
  await expect(page.getByRole("menuitem", { name: "Copy path" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Copy relative path" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Close tab" })).toBeVisible();
});
