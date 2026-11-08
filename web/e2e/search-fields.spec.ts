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
  await bare(page.getByRole("dialog", { name: "Choose a model" }).getByRole("textbox").first());
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

test("the thread find and diff find inputs keep their fill and padding", async ({ page }) => {
  await page.goto("/#/threads/t-done");
  await page.keyboard.press("ControlOrMeta+f");
  await filled(page.locator(".thread-find input").first());
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Show side panel" }).click();
  await page.getByRole("tab", { name: "Diff" }).first().click();
  await filled(page.locator(".diff-find input").first());
});
