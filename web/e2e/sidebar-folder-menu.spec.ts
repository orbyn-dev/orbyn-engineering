import { expect, test, type Page } from "@playwright/test";

/**
 * Characterization of the project folder's right-click menu (folderMenu.tsx, FolderHeader in listRows.tsx).
 * The mock's project-scope catalog gives every project its own folder, so a folder that merges several
 * projects (the case where "Grouping" lists more than one member) cannot be drawn here; the Grouping
 * menu is reached through a saved per-project override instead.
 */

const APP = "orbyn-engineering-app";
const folder = (page: Page, name = APP) => page.locator(".sidebar-scroll button.folder-toggle", { hasText: name });
const menu = (page: Page, name = APP) => page.getByRole("menu", { name: `Actions for ${name}` });

test.beforeEach(async ({ page }) => {
  // The clipboard is stubbed so the copy is read back without a permission prompt.
  await page.addInitScript(() => {
    const copied: string[] = [];
    Object.defineProperty(window, "__copied", { value: copied });
    Object.defineProperty(navigator, "clipboard", { value: { writeText: async (text: string) => void copied.push(text) }, configurable: true });
  });
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/threads/t-done");
  await expect(folder(page)).toBeVisible();
});

test.describe("folder menu", () => {
  test("a right-click opens the folder's menu with its three actions and no Grouping", async ({ page }) => {
    await folder(page).click({ button: "right" });
    await expect(menu(page)).toBeVisible();
    await expect(menu(page).getByRole("menuitem")).toHaveText(["New thread", "Copy path", "Project settings"]);
    // The right-click is not a click: the folder stays open.
    await expect(folder(page)).toHaveAttribute("aria-expanded", "true");
  });

  test("each folder's menu is named after it and acts on its project", async ({ page }) => {
    await folder(page, "marketing-site").click({ button: "right" });
    await expect(menu(page, "marketing-site")).toBeVisible();
    await menu(page, "marketing-site").getByRole("menuitem", { name: "Copy path" }).click();
    await expect.poll(() => page.evaluate(() => (window as unknown as { __copied: string[] }).__copied)).toEqual(["/Users/dev/code/marketing-site"]);
  });

  test("New thread opens a new thread in that project", async ({ page }) => {
    await folder(page).click({ button: "right" });
    await menu(page).getByRole("menuitem", { name: "New thread" }).click();
    await expect(page).toHaveURL(/#\/new\/p-app$/);
    await expect(menu(page)).toBeHidden();
  });

  test("Copy path copies the project's path and says so", async ({ page }) => {
    await folder(page).click({ button: "right" });
    await menu(page).getByRole("menuitem", { name: "Copy path" }).click();
    await expect(page.getByRole("status").filter({ hasText: "Copied path" })).toBeVisible();
    expect(await page.evaluate(() => (window as unknown as { __copied: string[] }).__copied)).toEqual(["/Users/dev/code/orbyn-engineering-app"]);
    // It copies and stays on the page.
    await expect(page).toHaveURL(/#\/threads\/t-done$/);
  });

  test("Project settings opens the Projects settings page", async ({ page }) => {
    await folder(page).click({ button: "right" });
    await menu(page).getByRole("menuitem", { name: "Project settings" }).click();
    await expect(page).toHaveURL(/#\/settings\/projects$/);
  });

  test("Escape closes the menu and returns focus to the folder", async ({ page }) => {
    await folder(page).click({ button: "right" });
    await expect(menu(page)).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(menu(page)).toBeHidden();
    await expect(folder(page)).toBeFocused();
  });

  test("the menu's own arrow keys walk its items", async ({ page }) => {
    await folder(page).click({ button: "right" });
    const items = menu(page).getByRole("menuitem");
    await expect(items.first()).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(items.nth(1)).toBeFocused();
    await page.keyboard.press("ArrowUp");
    await expect(items.first()).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/#\/new\/p-app$/);
  });
});

test.describe("Grouping submenu", () => {
  test.beforeEach(async ({ page }) => {
    // One saved override puts a Grouping entry in that project's folder menu.
    await page.addInitScript(() => {
      localStorage.setItem("orbyn.prefs.migrated", "1");
      localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ projectGroupingOverrides: { "p-app": "repository" } }));
    });
    await page.reload();
    await expect(folder(page)).toBeVisible();
  });

  const savedOverrides = (page: Page) =>
    page.evaluate(() => (JSON.parse(localStorage.getItem("orbyn.prefs.v1") ?? "{}") as { projectGroupingOverrides?: unknown }).projectGroupingOverrides);

  test("an overridden folder gains a Grouping entry after a separator; other folders do not", async ({ page }) => {
    await folder(page).click({ button: "right" });
    await expect(menu(page).getByRole("menuitem")).toHaveText(["New thread", "Copy path", "Project settings", "Grouping"]);
    await expect(menu(page).getByRole("menuitem", { name: "Grouping" })).toHaveAttribute("aria-haspopup", "menu");
    await expect(menu(page).locator("hr.menu-separator")).toHaveCount(1);
    await page.keyboard.press("Escape");

    await folder(page, "marketing-site").click({ button: "right" });
    await expect(menu(page, "marketing-site").getByRole("menuitem")).toHaveText(["New thread", "Copy path", "Project settings"]);
  });

  test("Grouping lists the folder's projects, each with the grouping modes and the current one marked", async ({ page }) => {
    await folder(page).click({ button: "right" });
    await menu(page).getByRole("menuitem", { name: "Grouping" }).click();
    const projects = page.getByRole("menu", { name: "Grouping", exact: true });
    await expect(projects.getByRole("menuitem")).toHaveText([APP]);
    await projects.getByRole("menuitem", { name: APP }).click();
    const modes = page.getByRole("menu", { name: APP, exact: true });
    await expect(modes.getByRole("menuitem")).toHaveText(["Default grouping", "Repository", "Repository path", "Separate"]);
    await expect(modes.getByRole("menuitem", { name: "Repository", exact: true })).toHaveAttribute("aria-current", "true");
    await expect(modes.getByRole("menuitem", { name: "Default grouping" })).not.toHaveAttribute("aria-current", "true");
    await expect(modes.getByRole("menuitem", { name: "Separate" })).not.toHaveAttribute("aria-current", "true");
  });

  test("choosing a mode saves it as that project's override and closes the menu", async ({ page }) => {
    await folder(page).click({ button: "right" });
    await menu(page).getByRole("menuitem", { name: "Grouping" }).click();
    await page.getByRole("menu", { name: "Grouping", exact: true }).getByRole("menuitem", { name: APP }).click();
    await page.getByRole("menu", { name: APP, exact: true }).getByRole("menuitem", { name: "Separate" }).click();
    await expect(menu(page)).toBeHidden();
    await expect.poll(() => savedOverrides(page)).toEqual({ "p-app": "separate" });
  });

  test("Default grouping removes the override, and with it the Grouping entry", async ({ page }) => {
    await folder(page).click({ button: "right" });
    await menu(page).getByRole("menuitem", { name: "Grouping" }).click();
    await page.getByRole("menu", { name: "Grouping", exact: true }).getByRole("menuitem", { name: APP }).click();
    await page.getByRole("menu", { name: APP, exact: true }).getByRole("menuitem", { name: "Default grouping" }).click();
    await expect.poll(() => savedOverrides(page)).toEqual({});
    await folder(page).click({ button: "right" });
    await expect(menu(page).getByRole("menuitem")).toHaveText(["New thread", "Copy path", "Project settings"]);
  });
});
