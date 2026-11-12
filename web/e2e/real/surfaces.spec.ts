import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { openPanelTab, startProject, type Service } from "./service";

/** Diff and file surfaces reading a real checkout through the real service. */

let service: Service;
let projectId = "";
let threadId = "";

const git = (...args: string[]) => execFileSync("git", args, { cwd: service.repo, stdio: "ignore" });
const write = (name: string, text: string) => writeFileSync(join(service.repo, name), text);

test.beforeAll(async () => {
  const started = await startProject({ fakeEngines: true });
  service = started.service;
  projectId = started.projectId;
  mkdirSync(join(service.repo, "tests"));
  write("notes.md", "# Notes\n\nSome **bold** text.\n");
  write("data.csv", 'name,count\n"Moon, the",1\nMars,2\n');
  write("page.html", "<h1>Page</h1><script>window.parent.document.title='pwned'</script>");
  write("flow.mmd", "graph TD\n  A --> B\n");
  write("book.ipynb", JSON.stringify({ cells: [{ cell_type: "markdown", source: ["# Cell title"] }, { cell_type: "code", source: ["print(1)"], outputs: [{ text: "1\n" }] }] }));
  write("tests/a.test.ts", "one\n");
  write("spaced.txt", "alpha beta\n");
  git("add", ".");
  git("-c", "user.name=T", "-c", "user.email=t@example.com", "commit", "-qm", "Second commit");
  write("README.md", "# Repo changed\n");
  write("tests/a.test.ts", "two\n");
  write("spaced.txt", "alpha   beta\n");
  write("fresh.txt", "new\n");
  git("add", "fresh.txt");
  const thread = await service.api<{ id: string }>("POST", "/api/threads", {
    project_id: projectId,
    engine: started.engines[0]?.id,
    model: null,
    title: "Surfaces",
    worktree: null,
    skill_profile: null,
  });
  threadId = thread.id;
});
test.afterAll(() => service.stop());

test("the Changes panel: letters, test filter, find, split, whitespace, collapse, wrap, commit scope", async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto(`${service.page}#/threads/${threadId}`);
  await openPanelTab(page, "Diff");
  const list = page.getByRole("group", { name: "Changed files" });
  const row = (name: RegExp) => list.getByRole("button", { name });
  await expect(row(/^Modified README\.md/).getByLabel("Modified")).toHaveText("M");
  await expect(row(/^Added fresh\.txt/).getByLabel("Added")).toHaveText("A");
  // Staged is not told apart here: fresh.txt is staged and sits in the one list with the rest.
  await expect(list.getByText(/^Staged/)).toHaveCount(0);

  /** A View options choice, then the menu closed. */
  const option = async (role: "menuitemcheckbox" | "menuitemradio", name: string) => {
    await page.getByRole("button", { name: "View options", exact: true }).click();
    await page.getByRole("menu", { name: "View options" }).getByRole(role, { name }).click();
    await page.keyboard.press("Escape");
  };

  await option("menuitemcheckbox", "Hide test files");
  await expect(row(/^Modified tests\/a\.test\.ts/)).toHaveCount(0);
  await option("menuitemcheckbox", "Hide test files");
  await expect(row(/^Modified tests\/a\.test\.ts/)).toHaveCount(1);

  await row(/^Modified README\.md/).click();
  await page.getByRole("group", { name: "Patch" }).focus();
  await page.keyboard.press("ControlOrMeta+f");
  await page.getByRole("searchbox", { name: "Find in diff" }).fill("repo changed");
  await expect(page.getByText("1 of 1")).toBeVisible();
  await expect(page.locator("mark.fhit").first()).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("searchbox", { name: "Find in diff" })).toHaveCount(0);

  // Split needs a wide panel, so the item is off until the panel is maximised.
  await page.getByRole("button", { name: "View options", exact: true }).click();
  await expect(page.getByRole("menuitemradio", { name: "Split" })).toBeDisabled();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Maximise panel", exact: true }).first().click();
  await option("menuitemradio", "Split");
  await expect(page.locator(".sp2").first()).toBeVisible();
  await option("menuitemradio", "Unified");
  await page.getByRole("button", { name: "Restore", exact: true }).first().click();

  await row(/^Modified spaced\.txt/).click();
  await option("menuitemcheckbox", "Ignore whitespace");
  await expect(page.locator("[data-diff-file='spaced.txt'] .l.add")).toHaveCount(0);
  await option("menuitemcheckbox", "Ignore whitespace");

  await option("menuitemcheckbox", "Collapse all files");
  await expect(page.getByRole("group", { name: "Patch" })).toHaveCount(0);
  await option("menuitemcheckbox", "Collapse all files");

  await row(/^Modified README\.md/).click();
  await option("menuitemcheckbox", "Wrap long lines");
  await expect(page.getByRole("group", { name: "Patch" })).toHaveAttribute("data-wrap", "off");
  await option("menuitemcheckbox", "Wrap long lines");

  await page.getByRole("button", { name: /^Scope: / }).click();
  await page.getByRole("menuitem", { name: "Commit", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Commit" })).toContainText("Second commit");
  await row(/^\w+ notes\.md/).click();
  await expect(page.locator("[data-diff-file='notes.md']")).toBeVisible();
});

test("the file picker works before saved keybindings arrive", async ({ page }) => {
  let releaseKeybindings = () => {};
  const keybindingsReady = new Promise<void>((resolve) => { releaseKeybindings = resolve; });
  await page.route("**/api/keybindings", async (route) => {
    await keybindingsReady;
    await route.continue();
  });
  try {
    await page.goto(`${service.page}#/threads/${threadId}`);
    await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();
    await page.keyboard.press("ControlOrMeta+p");
    const palette = page.getByRole("dialog", { name: "Command palette" });
    await expect(palette).toBeVisible();
    await palette.getByRole("combobox", { name: "Search" }).fill("notes.md");
    await palette.getByRole("button", { name: /notes.md/ }).click();
    await expect(page.locator(".file-preview strong")).toHaveText("bold");
  } finally {
    releaseKeybindings();
    await page.unrouteAll({ behavior: "wait" });
  }
});

test("files preview by kind, keep the source, and save safely", async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto(`${service.page}#/threads/${threadId}`);
  const open = async (name: string) => {
    await page.keyboard.press("ControlOrMeta+p");
    const box = page.getByRole("dialog", { name: "Command palette" }).getByRole("combobox", { name: "Search" });
    await box.fill(name);
    await page.getByRole("dialog", { name: "Command palette" }).getByRole("button", { name: new RegExp(name) }).click();
  };
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();

  await open("notes.md");
  await expect(page.locator(".file-preview strong")).toHaveText("bold");
  await page.getByRole("radio", { name: "Source" }).click();
  await expect(page.locator(".file-source")).toContainText("**bold**");

  await page.getByRole("radio", { name: "Edit" }).click();
  await page.getByRole("textbox", { name: "Edit notes.md" }).fill("# Edited\n");
  write("notes.md", "# Changed elsewhere\n");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("alert").filter({ hasText: "changed on disk" })).toBeVisible();
  expect(readFileSync(join(service.repo, "notes.md"), "utf8")).toBe("# Changed elsewhere\n");

  await open("data.csv");
  await expect(page.locator(".file-table td").first()).toHaveText("Moon, the");

  await open("flow.mmd");
  await expect(page.locator(".md-diagram svg")).toBeVisible({ timeout: 15_000 });

  await open("book.ipynb");
  await expect(page.locator(".file-notebook h2")).toHaveText("Cell title");

  await open("page.html");
  await expect(page.frameLocator("iframe.file-frame").getByRole("heading", { name: "Page" })).toBeVisible();
  await expect(page).not.toHaveTitle("pwned");
});

test("settings rows for background work and providers save", async ({ page }) => {
  await page.goto(`${service.page}#/settings/general`);
  await page.getByRole("radio", { name: "Saver", exact: true }).click();
  await page.getByRole("textbox", { name: "Base directory" }).fill("/tmp/projects");
  await page.getByRole("textbox", { name: "Base directory" }).blur();
  await expect
    .poll(async () => {
      const prefs = await service.api<{ key: string; value: unknown }[]>("GET", "/api/preferences");
      return [prefs.find((entry) => entry.key === "background_activity_profile")?.value, prefs.find((entry) => entry.key === "add_project_base_directory")?.value];
    })
    .toEqual(["saver", "/tmp/projects"]);
});
