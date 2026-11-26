import { expect, test, type Locator, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { openExplorer, send, settled, startProject, type Service } from "./service";

/**
 * F70 / #1060: a multi-repository thread's file tab of ANOTHER repository, through a real `orbynd` and the real page.
 *
 * The tab renders that repository's own Markdown preview, its source and its plain-text editor; a line comment and the
 * file chip name `<repository>/<path>`; Copy path is the file's absolute path, `<thread folder>/<name>/<path>`, since a
 * repository's worktree is a folder of the thread's task folder. This runs a real service with ORBYN_MULTI_REPO=1 and a
 * thread in two repositories whose same-named files differ, opens the second repository's file from the left bar's Files
 * explorer and asserts what the real UI draws and sends. The foreign tab actions are stacked from PR #1695: before it, a
 * foreign tab offered no file chip and its Copy path was off.
 */

test.setTimeout(120_000);

const WINDOW = { width: 1500, height: 900 };

const PRIMARY_README = "# Primary checkout\n\nOnly the primary repository says this.\n";
const SECOND_README = "# Second checkout\n\nOnly the second repository says this.\n";
const PRIMARY_NOTES = "primary notes\n\nOnly the primary repository's notes.\n";
const SECOND_NOTES = "second notes\n\nOnly the second repository's notes.\n";

interface RepoRow {
  id: string;
  name: string;
  path: string;
  trusted: boolean;
}
interface TaskFolder {
  root: string;
  branch: string;
  repos: { repo_id: string; name: string; branch: string; trusted: boolean }[];
}
interface ThreadView {
  id: string;
  cwd: string;
}

let service: Service;
let projectId = "";
let secondRepo = "";
let secondRepoId = "";
let threadId = "";
let cwd = "";
let primaryName = "";
let secondName = "";

const git = (folder: string, ...args: string[]) => execFileSync("git", args, { cwd: folder, stdio: "ignore" });

/** A real Git repository with one commit on main, beside the fixture project, holding the same-named files the primary also has. */
function makeRepo(parent: string, name: string): string {
  const path = join(parent, name);
  mkdirSync(path);
  writeFileSync(join(path, "notes.txt"), "# second base\n");
  writeFileSync(join(path, "README.md"), `# ${name}\n`);
  for (const args of [["init", "-q", "-b", "main"], ["add", "."], ["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "First"]]) {
    execFileSync("git", args, { cwd: path, stdio: "ignore" });
  }
  return path;
}

/** Registers and trusts one repository of the project through the real routes; a retry finds what it made already. */
async function trustRepository(path: string): Promise<RepoRow> {
  const view = await service.api<{ repositories: RepoRow[] }>("GET", `/api/projects/${projectId}/repos`);
  const found = view.repositories.find((entry) => entry.path === path);
  const repository = found ?? (await service.api<RepoRow>("POST", `/api/projects/${projectId}/repos/add-folder`, { path }));
  if (!repository.trusted) await service.api("POST", `/api/projects/${projectId}/repos/${repository.id}/trust`);
  return repository;
}

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ multiRepo: true, fakeEngines: true }));
  // The same-named file is in both repositories before the thread is cut, so each worktree starts with it. The primary's
  // copy has to be committed, since the fixture project is already a repository.
  writeFileSync(join(service.repo, "notes.txt"), "primary base\n");
  git(service.repo, "add", "notes.txt");
  git(service.repo, "-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "A same-named file");
  secondRepo = makeRepo(dirname(service.dataDir), "second-repo");
  secondRepoId = (await trustRepository(secondRepo)).id;

  // One thread in both repositories: one task folder, one worktree per repository on the thread's branch.
  const thread = await service.api<{ id: string }>("POST", `/api/projects/${projectId}/repo-threads`, {
    repos: [{ repo_id: projectId, base_branch: "main" }, { repo_id: secondRepoId, base_branch: "main" }],
    engine: "codex",
    model: null,
    permission_mode: "supervised",
  });
  threadId = thread.id;
  const folder = await service.api<TaskFolder>("GET", `/api/threads/${threadId}/task-folder`);
  const primary = folder.repos.find((repo) => repo.repo_id === projectId);
  const second = folder.repos.find((repo) => repo.repo_id === secondRepoId);
  if (!primary || !second) throw new Error("The task folder is missing a repository.");
  primaryName = primary.name;
  secondName = second.name;
  const primaryWorktree = join(folder.root, primaryName);
  const secondWorktree = join(folder.root, secondName);

  // The same names now say different things in the two checkouts, and each has one file the other does not, so a read or a
  // tab of the wrong repository cannot pass by accident.
  writeFileSync(join(primaryWorktree, "README.md"), PRIMARY_README);
  writeFileSync(join(primaryWorktree, "notes.txt"), PRIMARY_NOTES);
  writeFileSync(join(primaryWorktree, "primary-only.txt"), "primary\n");
  writeFileSync(join(secondWorktree, "README.md"), SECOND_README);
  writeFileSync(join(secondWorktree, "notes.txt"), SECOND_NOTES);
  writeFileSync(join(secondWorktree, "second-only.txt"), "second\n");

  // Copy path is built from the thread's own cwd (the task folder) and the repository's name, both served by the service;
  // the spec reads the cwd back rather than guessing it, and pins it to the task folder the worktrees live in.
  cwd = (await service.api<ThreadView>("GET", `/api/threads/${threadId}`)).cwd;
  expect(cwd).toBe(folder.root);
});
test.afterAll(() => service.stop());

/** A row's accessible name starts with its file name and continues with its state; match only the start. */
const named = (name: string) => new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`);

const sidebar = (page: Page) => page.getByRole("navigation", { name: "Workspace" });
const chooser = (page: Page) => sidebar(page).getByRole("button", { name: /^Repository:/ });
const chipShelf = (page: Page) => page.getByRole("list", { name: "Context chips", exact: true });

/**
 * Opens Files in the left bar, picks one of the thread's repositories in the explorer's own chip (the header has another
 * chip over the same choice, so the left bar's is scoped) and waits for that repository's tree. A file only the picked
 * repository has proves the tree was read for it and not left over from the other.
 */
async function pickRepository(page: Page, name: string, ownFile: string): Promise<Locator> {
  const tree = await openExplorer(page);
  await expect(chooser(page)).toBeVisible();
  await chooser(page).click();
  await page.getByRole("menuitem", { name, exact: true }).click();
  await expect(chooser(page)).toHaveAccessibleName(`Repository: ${name}`);
  await expect(tree.getByRole("treeitem", { name: named(ownFile) })).toBeVisible();
  return tree;
}

/** Opens one file row of the tree; a file of the repository the explorer shows opens as that repository's tab. */
const openFile = (tree: Locator, name: string) => tree.getByRole("treeitem", { name: named(name) }).click();

test("the foreign tab renders that repository's Markdown preview, its source and its plain-text editor", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  await page.goto(`${service.page}#/threads/${threadId}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();

  // Pick the second repository in the left bar and open its README.md: the tab says which repository it is.
  let tree = await pickRepository(page, secondName, "second-only.txt");
  await openFile(tree, "README.md");
  await expect(page.getByRole("tab", { name: `README.md (${secondName})`, exact: true })).toBeVisible();
  const viewer = page.locator(".files-viewer");
  await expect(viewer.getByRole("heading", { name: "Second checkout" })).toBeVisible();
  await expect(viewer.getByText("Only the second repository says this.")).toBeVisible();
  await expect(viewer.getByText("Only the primary repository says this.")).toHaveCount(0);

  // Its Source is the same repository's text, so the primary repository's line of the same file is not there.
  await viewer.getByRole("radio", { name: "Source" }).click();
  await expect(viewer.locator(".file-comment-source")).toContainText("Only the second repository says this.");
  await expect(viewer.locator(".file-comment-source")).not.toContainText("Only the primary repository says this.");

  // A plain text file of that repository is the editor, holding that repository's text, not the primary's.
  await openFile(tree, "notes.txt");
  await expect(page.getByRole("tab", { name: `notes.txt (${secondName})`, exact: true })).toBeVisible();
  await expect(viewer.getByRole("textbox", { name: "Edit notes.txt" })).toHaveValue(SECOND_NOTES);

  // The primary repository's file opens as the thread's ordinary tab, unchanged: no repository on the tab, its own file.
  tree = await pickRepository(page, primaryName, "primary-only.txt");
  await openFile(tree, "README.md");
  await expect(page.getByRole("tab", { name: "README.md", exact: true })).toBeVisible();
  await expect(page.locator(".ctx-title b")).toHaveText("README.md");
  await expect(viewer.getByRole("heading", { name: "Primary checkout" })).toBeVisible();
});

test("a foreign tab's line comment, file chip and Copy path name its repository", async ({ page, context }) => {
  // Copy path writes the real clipboard: the same permission the composer's clipboard specs grant.
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: new URL(service.page).origin });
  await page.setViewportSize(WINDOW);
  await page.goto(`${service.page}#/threads/${threadId}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();

  const tree = await pickRepository(page, secondName, "second-only.txt");
  await openFile(tree, "README.md");
  await expect(page.getByRole("tab", { name: `README.md (${secondName})`, exact: true })).toBeVisible();
  const viewer = page.locator(".files-viewer");
  await expect(viewer.getByRole("heading", { name: "Second checkout" })).toBeVisible();

  // A line comment names the tab's repository and file, in the comment's own label ...
  await viewer.getByRole("radio", { name: "Source" }).click();
  await viewer.getByRole("button", { name: "Comment on line 3", exact: true }).click();
  await viewer.getByRole("textbox", { name: "File comment" }).fill("Why does the other repository say this?");
  await viewer.getByRole("button", { name: "Add comment", exact: true }).click();
  await expect(viewer.getByRole("list", { name: "File comments" })).toContainText(`${secondName}/README.md:3`);
  await expect(page.getByRole("button", { name: `Review comment ${secondName}/README.md:3`, exact: true })).toBeVisible();

  // ... and Add file to message puts a chip naming the repository beside it.
  await viewer.getByRole("button", { name: "Add file to message", exact: true }).click();
  const fileChip = chipShelf(page).locator("li[data-kind=file] span[title]");
  await expect(fileChip).toHaveAttribute("title", `${secondName}/README.md`);
  await expect(fileChip).toHaveText(`${secondName}/README.md`);

  // Copy path is the file's absolute path: the thread's task folder, the repository's worktree folder, then the file.
  await page.getByRole("button", { name: "Tab menu", exact: true }).click();
  const copy = page.getByRole("menuitem", { name: "Copy path" });
  await expect(copy).toBeEnabled();
  await copy.click();
  await expect(page.getByText("Path copied", { exact: true })).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(`${cwd}/${secondName}/README.md`);

  // Both chips reach the prompt the engine is sent, so the message says which repository each file is in.
  await send(page, "explain these files");
  await settled(service, threadId);
  const messages = JSON.stringify(await service.api("GET", `/api/threads/${threadId}/messages?limit=10`));
  expect(messages).toContain(`Review on \`${secondName}/README.md:3\``);
  expect(messages).toContain(`File \`${secondName}/README.md\``);
  expect(messages).toContain("Only the second repository says this.");
  expect(messages).not.toContain("Only the primary repository says this.");

  // The primary repository's tab keeps its ordinary file chip: no repository on the label.
  const primaryTree = await pickRepository(page, primaryName, "primary-only.txt");
  await openFile(primaryTree, "README.md");
  await expect(page.getByRole("tab", { name: "README.md", exact: true })).toBeVisible();
  await page.locator(".files-viewer").getByRole("button", { name: "Add file to message", exact: true }).click();
  await expect(chipShelf(page).locator("li[data-kind=file] span[title]")).toHaveAttribute("title", "README.md");
});
