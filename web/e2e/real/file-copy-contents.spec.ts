import { expect, test } from "@playwright/test";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { startProject, type Service } from "./service";

/**
 * Copy contents in the file viewer's panel bar, through the real service and Chromium's real clipboard: the text that
 * reaches the clipboard is the file's bytes as the service reads them, not what the viewer happens to render.
 */

let service: Service;
let projectId: string;
test.beforeAll(async () => { ({ service, projectId } = await startProject({ fakeEngines: true })); });
test.afterAll(async () => { await service?.stop(); });

test("Copy contents puts the file's complete text on the clipboard and says so", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: new URL(service.page).origin });
  // Markdown opens as its rendered preview, so the clipboard text differs from anything visible: *only* the bytes match.
  const contents = "# Copy me\n\n- first *item*\n- second <b>item</b>\n\n```ts\nconst answer = 42;\n```\nlast line without a break";
  writeFileSync(join(service.repo, "copy-source.md"), contents);
  const thread = await service.api<{ id: string }>("POST", "/api/threads", {
    project_id: projectId, engine: "codex", model: null, title: "Copy contents", worktree: null, skill_profile: null,
  });
  await page.goto(`${service.page}#/threads/${thread.id}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();
  await page.keyboard.press("ControlOrMeta+p");
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await palette.getByRole("combobox", { name: "Search" }).fill("copy-source.md");
  await palette.getByRole("button", { name: /copy-source\.md/ }).first().click();

  const viewer = page.locator(".files-viewer");
  const copy = viewer.getByRole("button", { name: "Copy contents" });
  await expect(copy).toBeEnabled();
  // The controls the row already had sit beside it.
  await expect(viewer.getByRole("link", { name: "Save file" })).toBeVisible();
  await expect(viewer.getByRole("radio", { name: "Source" })).toBeVisible();

  await page.evaluate(() => navigator.clipboard.writeText("stale clipboard"));
  await copy.click();
  await expect(page.getByText("Contents copied")).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(contents);

  // A file changed on disk and reopened copies its new text, never a cached older one.
  const changed = "# Copy me\n\nchanged on disk\n";
  writeFileSync(join(service.repo, "copy-source.md"), changed);
  await page.reload();
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();
  await page.keyboard.press("ControlOrMeta+p");
  await palette.getByRole("combobox", { name: "Search" }).fill("copy-source.md");
  await palette.getByRole("button", { name: /copy-source\.md/ }).first().click();
  await expect(copy).toBeEnabled();
  await copy.click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(changed);
});
