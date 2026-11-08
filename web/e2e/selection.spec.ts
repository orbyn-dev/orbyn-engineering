import { expect, test, type Page } from "@playwright/test";

const selected = (page: Page) => page.evaluate(() => window.getSelection()?.toString() ?? "");

async function dragAcross(page: Page, selector: string) {
  await page.locator(selector).first().scrollIntoViewIfNeeded();
  const box = await page.locator(selector).first().boundingBox();
  if (!box) throw new Error(`${selector} has no box`);
  const y = box.y + Math.min(box.height / 2, 12);
  await page.mouse.move(box.x + 2, y);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - 2, y + Math.min(box.height - 4, 30), { steps: 8 });
  await page.mouse.up();
}

test.beforeEach(async ({ page }) => {
  await page.goto("/__mock/scenario/default");
  await page.goto("/#/threads/t-done");
  await expect(page.locator(".msg-bubble").first()).toBeVisible();
});

test("a drag across a settings page selects nothing", async ({ page }) => {
  await page.goto("/#/settings/projects");
  await expect(page.getByRole("heading").first()).toBeVisible();
  await page.evaluate(() => window.getSelection()?.removeAllRanges());
  // From the page heading to the bottom of the window, over the page's labels and rows.
  const heading = await page.getByRole("heading").first().boundingBox();
  if (!heading) throw new Error("no heading box");
  await page.mouse.move(heading.x + 1, heading.y + heading.height / 2);
  await page.mouse.down();
  await page.mouse.move(heading.x + 500, 700, { steps: 12 });
  await page.mouse.up();
  expect(await selected(page)).toBe("");
});

test("a drag over a message selects its text", async ({ page }) => {
  await page.evaluate(() => window.getSelection()?.removeAllRanges());
  await dragAcross(page, ".msg-bubble");
  expect((await selected(page)).trim().length).toBeGreaterThan(0);
});

test("fields stay selectable and editable", async ({ page }) => {
  const field = page.getByRole("textbox", { name: /^Message/ });
  await field.fill("hello there");
  await field.selectText();
  expect(await field.evaluate((node: HTMLTextAreaElement) => node.value.slice(node.selectionStart, node.selectionEnd))).toBe("hello there");
  await expect(field).toHaveCSS("user-select", "text");
});
