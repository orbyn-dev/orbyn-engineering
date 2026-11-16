import { expect, test, type Locator } from "@playwright/test";

/** A search or filter pill is one field: the input inside it has no fill, border or padding of its own. */
const bare = async (input: Locator) => {
  await expect(input).toBeVisible();
  const style = await input.evaluate((node) => {
    const css = getComputedStyle(node);
    return { border: css.borderTopWidth, bg: css.backgroundColor, pad: css.paddingLeft };
  });
  expect(style).toEqual({ border: "0px", bg: "rgba(0, 0, 0, 0)", pad: "0px" });
};

test.beforeEach(async ({ page }) => {
  await page.goto("/__mock/scenario/default");
});

test("the Keybindings find field is one pill", async ({ page }) => {
  await page.goto("/#/settings/keybindings");
  await bare(page.getByRole("textbox", { name: "Filter keybindings" }));
});

test("the model picker's search is one pill", async ({ page }) => {
  await page.goto("/#/new");
  await page.locator("[data-model-trigger]").first().click();
  await bare(page.getByRole("dialog", { name: "Choose a model" }).getByRole("combobox").first());
});

test("the thread search in the sidebar is one pill", async ({ page }) => {
  await page.goto("/#/threads/t-done");
  await page.getByRole("button", { name: "Search threads" }).click();
  await bare(page.locator(".search-field input, .set-find input, .palette-search input").first());
});

/** Fields that are themselves the filled input keep their fill and padding: the shared pill reset must not strip them. */
const filled = async (input: Locator) => {
  await expect(input).toBeVisible();
  const style = await input.evaluate((node) => {
    const css = getComputedStyle(node);
    return { bg: css.backgroundColor, pad: css.paddingLeft };
  });
  expect(style.bg).not.toBe("rgba(0, 0, 0, 0)");
  expect(style.pad).not.toBe("0px");
};

test("the thread find input keeps its fill and padding", async ({ page }) => {
  await page.goto("/#/threads/t-done");
  await page.keyboard.press("ControlOrMeta+f");
  await filled(page.locator(".thread-find input").first());
});

test("the Changes file filter and find bar are one pill each", async ({ page }) => {
  await page.goto("/#/threads/t-approval");
  await page.getByRole("button", { name: "Show side panel" }).click();
  await page.getByRole("button", { name: "Open a tab" }).click();
  await page.getByRole("menuitem", { name: "Changes" }).click();
  await bare(page.getByRole("searchbox", { name: "Filter files" }));
  await page.getByRole("group", { name: "Patch" }).focus();
  await page.keyboard.press("ControlOrMeta+f");
  await bare(page.getByRole("searchbox", { name: "Find in diff" }));
});
