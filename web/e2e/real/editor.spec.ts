import { expect, test, type Page } from "@playwright/test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { startProject, type Service } from "./service";

let service: Service;
let projectId: string;
test.beforeAll(async () => { ({ service, projectId } = await startProject({ fakeEngines: true })); });
test.afterAll(async () => { await service?.stop(); });

/** A file of the thread's own checkout, opened from the command palette into the editor, through the real write route. */
async function openInEditor(page: Page, name: string, text: string) {
  writeFileSync(join(service.repo, name), text);
  const thread = await service.api<{ id: string }>("POST", "/api/threads", {
    project_id: projectId, engine: "codex", model: null, title: "Editor", worktree: null, skill_profile: null,
  });
  await page.goto(`${service.page}#/threads/${thread.id}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();
  await page.keyboard.press("ControlOrMeta+p");
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await palette.getByRole("combobox", { name: "Search" }).fill(name);
  await palette.getByRole("button", { name: new RegExp(name.replace(".", "\\.")) }).first().click();
  const code = page.getByRole("textbox", { name: `Edit ${name}` });
  await expect(code).toHaveValue(text);
  return code;
}

test("an edit is saved to the file on disk, and nothing is written before Save", async ({ page }) => {
  const code = await openInEditor(page, "edit-me.ts", "export const a = 1\n");
  await code.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("export const b = 2\n");
  await expect(page.locator(".ed-unsaved")).toHaveText("Unsaved");
  expect(readFileSync(join(service.repo, "edit-me.ts"), "utf8")).toBe("export const a = 1\n");
  await page.keyboard.press("ControlOrMeta+s");
  await expect(page.locator(".ed-end")).toHaveText("Saved");
  expect(readFileSync(join(service.repo, "edit-me.ts"), "utf8")).toBe("export const a = 1\nexport const b = 2\n");
});

test("a file that changed on disk asks: Keep mine overwrites it, and nothing is chosen before that", async ({ page }) => {
  const code = await openInEditor(page, "conflict.ts", "one\n");
  await code.click();
  await page.keyboard.type("mine ");
  const mine = await code.inputValue();
  expect(mine).toContain("mine ");
  writeFileSync(join(service.repo, "conflict.ts"), "theirs\n");
  await page.getByRole("button", { name: "Save" }).click();
  const banner = page.getByRole("alert").filter({ hasText: "Changed on disk" });
  await expect(banner).toBeVisible();
  expect(readFileSync(join(service.repo, "conflict.ts"), "utf8")).toBe("theirs\n");
  await expect(code).toHaveValue(mine);
  await banner.getByRole("button", { name: "Keep mine" }).click();
  await expect(page.locator(".ed-end")).toHaveText("Saved");
  expect(readFileSync(join(service.repo, "conflict.ts"), "utf8")).toBe(mine);
});

test("Reload takes what is on disk and drops the unsaved changes", async ({ page }) => {
  const code = await openInEditor(page, "reload.ts", "one\n");
  await code.click();
  await page.keyboard.type("mine ");
  writeFileSync(join(service.repo, "reload.ts"), "theirs\n");
  await page.getByRole("button", { name: "Save" }).click();
  await page.getByRole("alert").getByRole("button", { name: "Reload" }).click();
  await expect(code).toHaveValue("theirs\n");
  await expect(page.locator(".ed-end")).toHaveText("Saved");
});
