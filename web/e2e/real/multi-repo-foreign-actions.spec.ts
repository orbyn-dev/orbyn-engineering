import { expect, test, type Locator, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { openExplorer, send, settled, startProject, type Service } from "./service";

/**
 * F70 / #1060: a multi-repository thread's file tab of ANOTHER repository, through a real `orbynd` and the real page.
 *
 * The tab renders that repository's own Markdown preview, its source, its plain-text editor and its own media: an image
 * of the second repository is drawn from that repository's worktree. A line comment and the file chip name
 * `<repository>/<path>`; Copy path is the file's absolute path, `<thread folder>/<name>/<path>`, since a repository's
 * worktree is a folder of the thread's task folder. This runs a real service with ORBYN_MULTI_REPO=1 and a
 * thread in two repositories whose same-named files differ, opens the second repository's file from the left bar's Files
 * explorer and asserts what the real UI draws and sends. The foreign tab actions are stacked from PR #1695: before it, a
 * foreign tab offered no file chip and its Copy path was off. The primary repository's own tab names that repository (PR
 * #1706), while an ordinary single-repository thread's tab is still named by its file alone.
 */

test.setTimeout(120_000);

const WINDOW = { width: 1500, height: 900 };

const PRIMARY_README = "# Primary checkout\n\nOnly the primary repository says this.\n";
const SECOND_README = "# Second checkout\n\nOnly the second repository says this.\n";
const PRIMARY_NOTES = "primary notes\n\nOnly the primary repository's notes.\n";
const SECOND_NOTES = "second notes\n\nOnly the second repository's notes.\n";

// Two real PNGs whose decoded pixel sizes differ (2x3 in the second repository, 3x2 in the primary), so a tab that drew
// the wrong repository's file, a stale copy of the other one or a broken element could not pass: the element reports the
// pixels it actually decoded, and the bytes the service answers are compared with these exact files.
const FOREIGN_IMAGE = "second-only.png";
const PRIMARY_IMAGE = "primary-only.png";
const FOREIGN_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAADCAIAAAA2iEnWAAAAFElEQVR4nGO8IyfHwMDAxAAGUAoAFdoBHtT31FUAAAAASUVORK5CYII=",
  "base64",
);
const PRIMARY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAIAAAASFvFNAAAAFElEQVR4nGOUs7nDAAZMEIqBgQEAFUoBOt09YecAAAAASUVORK5CYII=",
  "base64",
);
const FOREIGN_SIZE = { width: 2, height: 3 };
const PRIMARY_SIZE = { width: 3, height: 2 };

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
  // One real image in each checkout, with different pixels: the foreign image exists only in the second repository, so
  // the primary cannot answer for it, and its rendered size is the second file's size.
  writeFileSync(join(secondWorktree, FOREIGN_IMAGE), FOREIGN_PNG);
  writeFileSync(join(primaryWorktree, PRIMARY_IMAGE), PRIMARY_PNG);

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

  // The primary repository's file opens as the thread's own tab that names the repository (#1706), while its content stays
  // the primary repository's own.
  tree = await pickRepository(page, primaryName, "primary-only.txt");
  await openFile(tree, "README.md");
  await expect(page.getByRole("tab", { name: `README.md (${primaryName})`, exact: true })).toBeVisible();
  await expect(page.locator(".ctx-title b")).toHaveText(`README.md (${primaryName})`);
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

  // The primary repository's tab names it too (#1706), and its chip carries that name: the send above took the foreign
  // chip with it, so the shelf holds this one chip alone.
  const primaryTree = await pickRepository(page, primaryName, "primary-only.txt");
  await openFile(primaryTree, "README.md");
  await expect(page.getByRole("tab", { name: `README.md (${primaryName})`, exact: true })).toBeVisible();
  await page.locator(".files-viewer").getByRole("button", { name: "Add file to message", exact: true }).click();
  const primaryChip = chipShelf(page).locator("li[data-kind=file] span[title]");
  await expect(primaryChip).toHaveCount(1);
  await expect(primaryChip).toHaveAttribute("title", `${primaryName}/README.md`);
});

test("a foreign image tab renders the second repository's own PNG, and the primary is unaffected", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  await page.goto(`${service.page}#/threads/${threadId}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();

  // The second repository's explorer lists the image only it has, and opening it asks the raw route for that repository.
  const secondTree = await pickRepository(page, secondName, "second-only.txt");
  await expect(secondTree.getByRole("treeitem", { name: named(FOREIGN_IMAGE) })).toBeVisible();
  const raw = page.waitForResponse((answer) => {
    const url = new URL(answer.url());
    return url.pathname === `/api/threads/${threadId}/files/raw` && url.searchParams.get("path") === FOREIGN_IMAGE;
  });
  await openFile(secondTree, FOREIGN_IMAGE);
  await expect(page.getByRole("tab", { name: `${FOREIGN_IMAGE} (${secondName})`, exact: true })).toBeVisible();

  // The tab's image element names the second repository, and the service answers that repository's exact file. The same
  // path without a repository is a 404, so the rendered bytes cannot be the primary repository's.
  const viewer = page.locator(".files-viewer");
  const foreignImage = viewer.getByRole("img", { name: FOREIGN_IMAGE });
  await expect(foreignImage).toBeVisible();
  const src = new URL((await foreignImage.getAttribute("src")) ?? "", service.page);
  expect(src.pathname).toBe(`/api/threads/${threadId}/files/raw`);
  expect(src.searchParams.get("path")).toBe(FOREIGN_IMAGE);
  expect(src.searchParams.get("repo_id")).toBe(secondRepoId);
  const answer = await raw;
  expect(answer.status()).toBe(200);
  expect(Buffer.from(await answer.body()).equals(FOREIGN_PNG)).toBe(true);
  const missingFromPrimary = await service
    .api("GET", `/api/threads/${threadId}/files/raw?path=${FOREIGN_IMAGE}`)
    .then(() => "the primary served the foreign image", (failure: unknown) => String(failure));
  expect(missingFromPrimary).toContain("404");

  // A broken element reports no pixels; this one decoded the second repository's 2x3 PNG.
  await expect.poll(() => foreignImage.evaluate((element: HTMLImageElement) => ({ complete: element.complete, width: element.naturalWidth, height: element.naturalHeight }))).toEqual({ complete: true, ...FOREIGN_SIZE });

  // The primary repository is unaffected: its tree has no foreign image, and its own PNG opens from the thread's checkout
  // as the tab that names the primary repository (#1706), with no repository on the raw request.
  const primaryTree = await pickRepository(page, primaryName, "primary-only.txt");
  await expect(primaryTree.getByRole("treeitem", { name: named(FOREIGN_IMAGE) })).toHaveCount(0);
  await openFile(primaryTree, PRIMARY_IMAGE);
  await expect(page.getByRole("tab", { name: `${PRIMARY_IMAGE} (${primaryName})`, exact: true })).toBeVisible();
  const ownImage = viewer.getByRole("img", { name: PRIMARY_IMAGE });
  await expect(ownImage).toBeVisible();
  const ownSrc = new URL((await ownImage.getAttribute("src")) ?? "", service.page);
  expect(ownSrc.searchParams.get("path")).toBe(PRIMARY_IMAGE);
  expect(ownSrc.searchParams.get("repo_id")).toBeNull();
  await expect.poll(() => ownImage.evaluate((element: HTMLImageElement) => ({ complete: element.complete, width: element.naturalWidth, height: element.naturalHeight }))).toEqual({ complete: true, ...PRIMARY_SIZE });
});

test("the primary repository's file tab names it, while an ordinary thread's tab keeps its file name", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  await page.goto(`${service.page}#/threads/${threadId}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();

  // #1706: in a multi-repository thread the primary repository's file opens from the explorer as a tab that names the
  // repository, exactly as the panel's own naming does: "<file> (<repository>)".
  const tree = await pickRepository(page, primaryName, "primary-only.txt");
  await openFile(tree, "README.md");
  await expect(page.getByRole("tab", { name: `README.md (${primaryName})`, exact: true })).toBeVisible();
  await expect(page.locator(".ctx-title b")).toHaveText(`README.md (${primaryName})`);
  // The tab still reads as the thread's own: the preview is the primary repository's file, not another repository's.
  await expect(page.locator(".files-viewer").getByRole("heading", { name: "Primary checkout" })).toBeVisible();

  // Another repository's tab still names that repository, so the two same-named files are told apart by their tabs.
  const foreignTree = await pickRepository(page, secondName, "second-only.txt");
  await openFile(foreignTree, "README.md");
  await expect(page.getByRole("tab", { name: `README.md (${secondName})`, exact: true })).toBeVisible();
  await expect(page.getByRole("tab", { name: `README.md (${primaryName})`, exact: true })).toBeVisible();

  // An ordinary thread in the same project keeps the file tab it always had: the command palette opens its file as the
  // thread's own tab, named by the file alone, and there is no repository chooser because it works in one repository.
  const ordinary = await service.api<{ id: string }>("POST", "/api/threads", { project_id: projectId, engine: "codex", title: "Ordinary thread" });
  await page.goto(`${service.page}#/threads/${ordinary.id}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Repository:/ })).toHaveCount(0);
  await page.keyboard.press("ControlOrMeta+p");
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await palette.getByRole("combobox", { name: "Search" }).fill("README.md");
  await palette.getByRole("button", { name: /README\.md/ }).first().click();
  await expect(page.getByRole("tab", { name: "README.md", exact: true })).toBeVisible();
  await expect(page.locator(".ctx-title b")).toHaveText("README.md");
  await expect(page.locator(".files-viewer").getByRole("heading", { name: "Repo" })).toBeVisible();
});
