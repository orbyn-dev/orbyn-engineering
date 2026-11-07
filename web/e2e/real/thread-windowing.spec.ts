import { expect, test } from "@playwright/test";
import { settled, startProject, type Service } from "./service";

/**
 * A long transcript mounts only the rows near the view. 40 turns of the `chunk` scene are
 * 2,000 log items; the page must stay small, while scroll, find and Jump to latest still
 * reach rows that are nowhere near the window.
 */
const TURNS = 40;
let service: Service;
let projectId = "";
let engine = "";

test.beforeAll(async () => {
  test.setTimeout(300_000);
  const started = await startProject({ scenes: true });
  ({ service, projectId } = started);
  engine = started.engines.find((e) => /codex/i.test(e.id) || /codex/i.test(e.name))?.id ?? started.engines[0]!.id;
});
test.afterAll(() => service.stop());

async function seed(): Promise<string> {
  const thread = await service.api<{ id: string }>("POST", "/api/threads", { project_id: projectId, engine, model: null, title: "Long", worktree: null, skill_profile: null });
  for (let n = 0; n < TURNS; n++) {
    const tag = n === 0 || n === TURNS - 1 ? "alpha-needle" : `turn ${n}`;
    await service.api("POST", `/api/threads/${thread.id}/runs`, { prompt: `[scene:chunk] ${tag}` });
    await settled(service, thread.id);
  }
  return thread.id;
}

const mountedRows = (page: import("@playwright/test").Page) => page.locator(".transcript .item").count();
const nodes = (page: import("@playwright/test").Page) => page.evaluate(() => document.querySelectorAll(".transcript *").length);

test("a long thread stays small in the page, and scroll, find and Jump to latest reach far rows", async ({ page }) => {
  test.setTimeout(300_000);
  const id = await seed();
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto(`${service.page}#/threads/${id}`);
  const log = page.getByRole("log", { name: "Thread messages" });
  await expect(log).toBeVisible();

  // Opens at the end; only a window of the 120 rows is in the page.
  await expect(page.getByText("Reply 24").last()).toBeVisible({ timeout: 30_000 });
  expect(await mountedRows(page)).toBeLessThan(45);
  expect(await nodes(page)).toBeLessThan(4000);
  await expect(log.getByText("alpha-needle").first()).toBeVisible();

  // Scrolling to the top brings the first prompt in and lets go of the end.
  await log.evaluate((element) => { element.scrollTop = 0; });
  await expect(log.locator(".msg-user", { hasText: "alpha-needle" }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Jump to latest" })).toBeVisible();
  expect(await mountedRows(page)).toBeLessThan(45);

  // Jump to latest goes back across the whole thread.
  await page.getByRole("button", { name: "Jump to latest" }).click();
  await expect(page.getByText("Reply 24").last()).toBeVisible();
  await expect(page.getByRole("button", { name: "Jump to latest" })).toBeHidden();

  // Find counts rows that are not mounted and steps to each, near or far.
  await page.keyboard.press("ControlOrMeta+f");
  await page.getByRole("textbox", { name: "Find in thread" }).fill("alpha-needle");
  await expect(page.locator(".thread-find-count")).toHaveText("1/2");
  await expect(log.locator(".msg-user", { hasText: "alpha-needle" }).first()).toBeVisible();
  expect(await mountedRows(page)).toBeLessThan(45);
  await page.getByRole("button", { name: "Next match" }).click();
  await expect(page.locator(".thread-find-count")).toHaveText("2/2");
  const last = log.locator(".msg-user", { hasText: "alpha-needle" }).last();
  await expect(last).toBeVisible();
  const box = await last.boundingBox();
  const view = await log.boundingBox();
  expect(box!.y).toBeGreaterThanOrEqual(view!.y);
  expect(box!.y + box!.height).toBeLessThanOrEqual(view!.y + view!.height);
  expect(await mountedRows(page)).toBeLessThan(45);
});
