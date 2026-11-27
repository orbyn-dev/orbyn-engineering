import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { projectSettingsUrl, startProject, type Service } from "./service";

/**
 * F70 second slice: the Settings › Projects › Repositories writes and layout, through a real `orbynd` with
 * `ORBYN_MULTI_REPO=1`.
 *
 * The first F70 spec (web/e2e/real/multi-repo.spec.ts) covers the list, Add a folder, Trust, the repo thread and its task
 * folder. This one covers what its row still names as unrun: creating a folder (the service makes it under the clone
 * directory and runs `git init`, and the row starts trusted), Remove from project (the row goes, the folder on disk
 * stays), Set up Git on a trusted folder that has none, the narrow-window stacking layout, publish's 422 unsupported
 * answer shown as Planned, and the header Repos chip opened as a phone sheet. Every write is one dialog confirm, and both
 * the page and the service's own state are asserted.
 */

test.describe.configure({ mode: "serial" });
test.setTimeout(90_000);

const WIDE = { width: 1500, height: 1000 };
const NARROW = { width: 700, height: 1000 };
const PHONE = { width: 390, height: 844 };

interface RepoRow {
  id: string;
  name: string;
  path: string;
  is_git: boolean;
  trusted: boolean;
  branch: string | null;
  branches: string[];
  has_remote: boolean;
  primary: boolean;
  clone: { state: string; percent: number | null; message?: string | null } | null;
}
interface ReposView {
  repositories: RepoRow[];
  clone_directory: string | null;
  max_per_thread: number;
}
interface TaskFolder {
  root: string;
  branch: string;
  repos: { repo_id: string; name: string; branch: string; trusted: boolean }[];
  max_per_thread: number;
}

let service: Service;
let projectId = "";
let secondRepo = "";
let plainFolder = "";
let threadId = "";
let threadFolder: TaskFolder;
let remoteBare = "";

/**
 * The ssh program the service's Git runs for the clone tests. The service refuses `file://` and local paths by design
 * (docs/multi-repo.md, "Clone and path input") and runs clones with `GIT_ALLOW_PROTOCOL=https:ssh`, so a real local
 * remote is reached over an ssh source whose ssh program is this stand-in, as the repo's forge specs stand in for `gh`:
 * it execs `git-upload-pack` on the folder the source names, and never answers for the host `stalled.test`, so that
 * clone stays running long enough to be cancelled. The source names a folder, never the fixture's path: a clone source
 * cannot hold whitespace, and the host temporary directory can name one on Windows.
 */
const remoteFolder = "clone-remote.git";
const fakeSsh = (remote: string) => `#!/usr/bin/env node
const { execFileSync } = require("node:child_process");
const args = process.argv.slice(2);
const host = (args.find((arg) => arg.includes("@")) || "").split("@").pop();
const remote = ${JSON.stringify(remote)};
if (host === "stalled.test") {
  // A clone that never answers: the spec cancels it while it runs.
  setTimeout(() => process.exit(0), 120000);
} else {
  const last = args[args.length - 1] || "";
  const match = last.match(/git-upload-pack '?([^']+)'?/);
  if (!match) { console.error("fake-ssh: unexpected remote command: " + last); process.exit(1); }
  execFileSync("git-upload-pack", [match[1] === ${JSON.stringify(remoteFolder)} ? remote : match[1]], { stdio: "inherit" });
}
`;

/** A real Git repository with one commit on main, beside the fixture project. */
function makeRepo(parent: string, name: string): string {
  const path = join(parent, name);
  mkdirSync(path);
  writeFileSync(join(path, "README.md"), `# ${name}\n`);
  for (const args of [["init", "-q", "-b", "main"], ["add", "."], ["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "First"]]) {
    execFileSync("git", args, { cwd: path, stdio: "ignore" });
  }
  return path;
}

const branchOf = (cwd: string) => execFileSync("git", ["symbolic-ref", "--short", "HEAD"], { cwd }).toString().trim();

/** The local bare remote as an ssh source, named by a folder the stand-in ssh program maps to the real one. */
const sshSource = (host: string) => `git@${host}:${remoteFolder}`;

/** Registers and trusts one folder of the project through the real routes. */
async function registerFolder(path: string): Promise<RepoRow> {
  const view = await service.api<ReposView>("GET", `/api/projects/${projectId}/repos`);
  const found = view.repositories.find((entry) => entry.path === path);
  const repository = found ?? (await service.api<RepoRow>("POST", `/api/projects/${projectId}/repos/add-folder`, { path }));
  if (!repository.trusted) await service.api("POST", `/api/projects/${projectId}/repos/${repository.id}/trust`);
  return repository;
}

/** The project's repository list as the service answers it. */
const repos = () => service.api<ReposView>("GET", `/api/projects/${projectId}/repos`);
const rowNamed = async (name: string) => (await repos()).repositories.find((entry) => entry.name === name);

const repositories = (page: Page) => page.getByRole("region", { name: "Repositories" });
const row = (page: Page, name: string) => repositories(page).getByRole("listitem").filter({ hasText: name });

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ multiRepo: true, fakeEngines: true }));
  const parent = dirname(service.dataDir);
  // A second Git folder, trusted, so the project has two usable repositories and a thread can be started in both.
  secondRepo = makeRepo(parent, "second-repo");
  const second = await registerFolder(secondRepo);
  // A plain folder with a file and no Git: Add a folder brings it in untrusted, Trust then offers Set up Git.
  plainFolder = join(parent, "learning-notes");
  mkdirSync(plainFolder);
  writeFileSync(join(plainFolder, "notes.md"), "# Notes\n");
  // The thread whose header chip opens as a sheet on a phone; a thread needs two trusted repositories.
  const thread = await service.api<{ id: string }>("POST", `/api/projects/${projectId}/repo-threads`, {
    repos: [{ repo_id: projectId, base_branch: "main" }, { repo_id: second.id, base_branch: "main" }],
    engine: "codex",
    model: null,
    permission_mode: "supervised",
  });
  threadId = thread.id;
  threadFolder = await service.api<TaskFolder>("GET", `/api/threads/${threadId}/task-folder`);
  // A real local Git remote for the clone tests: the fixture repository, cloned bare. The service's Git runs the ssh
  // program written below from its own HOME, so a clone source is an ssh address that Git takes, and the remote is real.
  const cloneSource = makeRepo(parent, "clone-source");
  remoteBare = join(parent, remoteFolder);
  execFileSync("git", ["clone", "-q", "--bare", cloneSource, remoteBare], { stdio: "ignore" });
  const home = dirname(service.dataDir);
  writeFileSync(join(home, "fake-ssh"), fakeSsh(remoteBare), { mode: 0o755 });
  // Git runs core.sshCommand through the shell, which splits an unquoted path at a space in the host temporary
  // directory; escaped quotes keep it one argument. A backslash in a config value starts an escape, which makes a
  // Windows path a syntax error, so forward slashes name the same file on both platforms.
  const sshProgram = join(home, "fake-ssh").replace(/\\/g, "/");
  writeFileSync(join(home, ".gitconfig"), `[core]\n\tsshCommand = \\"${sshProgram}\\"\n`);
});

test.afterAll(() => service.stop());
test.beforeEach(async ({ page }) => {
  await page.setViewportSize(WIDE);
});

test("Create a new folder makes it under the clone directory and Remove from project keeps its files", async ({ page }) => {
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section).toBeVisible();
  await expect(section.getByRole("listitem")).toHaveCount(2);

  // One dialog confirm, one write: the service picks the place, makes the folder and runs git init; the row starts trusted.
  await section.getByRole("button", { name: "Add repository" }).click();
  await page.getByRole("menuitem", { name: "Create a new folder…" }).click();
  const create = page.getByRole("dialog", { name: "Create a repository" });
  await expect(create).toContainText("It starts trusted");
  await create.getByRole("textbox", { name: "Folder name" }).fill("field-notes");
  await create.getByRole("button", { name: "Create", exact: true }).click();
  await expect(section.getByRole("listitem")).toHaveCount(3);
  const created = row(page, "field-notes");
  const view = await repos();
  const made = view.repositories.find((entry) => entry.name === "field-notes");
  if (!made) throw new Error("The service did not list the created folder.");
  // The service's own row: a managed Git folder under the reported clone directory, trusted, not the primary, branch main.
  expect(view.clone_directory).not.toBeNull();
  expect(dirname(made.path)).toBe(view.clone_directory);
  expect(basename(made.path)).toBe("field-notes");
  expect([made.is_git, made.trusted, made.primary, made.branch, made.has_remote, made.clone]).toEqual([true, true, false, "main", false, null]);
  await expect(created).toContainText(made.path);
  await expect(created.locator(".repos-branch")).toHaveText("main");
  await expect(created.getByText("Trusted", { exact: true })).toBeVisible();
  // The folder is on disk with its own Git repository, on its initial branch.
  expect(existsSync(made.path)).toBe(true);
  expect(existsSync(join(made.path, ".git"))).toBe(true);
  expect(branchOf(made.path)).toBe("main");

  // Remove stops tracking it: the dialog says the files stay, the row and the service's entry go, the folder does not.
  await created.getByRole("button", { name: "Actions for field-notes" }).click();
  await page.getByRole("menuitem", { name: "Remove from project…" }).click();
  const remove = page.getByRole("dialog", { name: /^Remove field-notes from / });
  await expect(remove).toContainText("Orbyn stops tracking it in this project. Files on disk stay, and threads keep their history.");
  await remove.getByRole("button", { name: "Remove", exact: true }).click();
  await expect(section.getByRole("listitem")).toHaveCount(2);
  await expect(row(page, "field-notes")).toHaveCount(0);
  expect(await rowNamed("field-notes")).toBeUndefined();
  expect(existsSync(made.path)).toBe(true);
  expect(branchOf(made.path)).toBe("main");
});

test("Set up Git runs git init on a trusted folder that has none", async ({ page }) => {
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section).toBeVisible();
  const before = await repos();
  expect(before.repositories.map((entry) => entry.name)).toEqual(["my-repo", "second-repo"]);

  // Add a folder in place: it arrives untrusted, so the page offers Trust and not Set up Git yet.
  await section.getByRole("button", { name: "Add repository" }).click();
  await page.getByRole("menuitem", { name: "Add a folder…" }).click();
  const add = page.getByRole("dialog", { name: "Add a folder" });
  await add.getByRole("textbox", { name: "Folder on this computer" }).fill(plainFolder);
  await add.getByRole("button", { name: "Add", exact: true }).click();
  const notes = row(page, "learning-notes");
  await expect(notes.getByText("Not trusted", { exact: true })).toBeVisible();
  await expect(notes.getByRole("button", { name: "Set up Git" })).toHaveCount(0);
  const added = await rowNamed("learning-notes");
  if (!added) throw new Error("The service did not list the added folder.");
  expect([added.is_git, added.trusted, added.branch]).toEqual([false, false, null]);

  // Trust is its own write; only then does the row show the folder has no Git and offer Set up Git.
  await notes.getByRole("button", { name: "Trust…" }).click();
  await page.getByRole("dialog", { name: "Trust learning-notes?" }).getByRole("button", { name: "Trust folder" }).click();
  await expect(notes.getByText("Folder", { exact: true })).toBeVisible();
  await expect(notes.getByText("Not a Git repository", { exact: true })).toBeVisible();
  const trusted = await rowNamed("learning-notes");
  if (!trusted) throw new Error("The service did not keep the trusted folder.");
  expect([trusted.is_git, trusted.trusted]).toEqual([false, true]);

  // One confirm, one write: git init, reported as a Git folder on its initial branch.
  await notes.getByRole("button", { name: "Set up Git", exact: true }).click();
  const setup = page.getByRole("dialog", { name: "Set up Git in learning-notes?" });
  await expect(setup).toContainText(plainFolder);
  await setup.getByRole("button", { name: "Set up Git", exact: true }).click();
  await expect(notes.getByText("Trusted", { exact: true })).toBeVisible();
  await expect(notes.locator(".repos-branch")).toHaveText("main");
  const made = await rowNamed("learning-notes");
  if (!made) throw new Error("The service did not list the folder after setting up Git.");
  expect([made.is_git, made.trusted, made.branch]).toEqual([true, true, "main"]);
  expect(existsSync(join(plainFolder, ".git"))).toBe(true);
  expect(branchOf(plainFolder)).toBe("main");
  // The file the folder already had is still there: setup-git wrote only .git.
  expect(existsSync(join(plainFolder, "notes.md"))).toBe(true);
});

test("Publish answers 422 unsupported: the page shows Planned, not success and not an error", async ({ page }) => {
  // A trusted repository with no remote is what offers Publish; created here so the test does not depend on the first one.
  const published = await service.api<RepoRow>("POST", `/api/projects/${projectId}/repos/create`, { name: "publish-me" });
  expect([published.trusted, published.has_remote]).toEqual([true, false]);
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section).toBeVisible();
  const target = row(page, "publish-me");
  await expect(target).toBeVisible();

  // Step 1 is the page's own provider choice; only GitHub is ready.
  await target.getByRole("button", { name: "Actions for publish-me" }).click();
  await page.getByRole("menuitem", { name: "Publish to GitHub…" }).click();
  const dialog = page.getByRole("dialog", { name: "Publish repository" });
  await expect(dialog).toContainText("Step 1 of 3 · Provider");
  await dialog.getByRole("radio", { name: /GitHub/ }).click();
  await dialog.getByRole("button", { name: "Next" }).click();
  await expect(dialog).toContainText("Step 2 of 3 · Repository");
  await expect(dialog.getByRole("textbox", { name: "Repository" })).toHaveValue("orbyn-dev/publish-me");
  await dialog.getByRole("button", { name: "Publish", exact: true }).click();

  // The service's 422 unsupported is shown as the Planned state with its own words: no step 3, no success, no alert.
  await expect(dialog.getByText("Planned", { exact: true })).toBeVisible();
  await expect(dialog.getByText("Publishing a repository arrives in a later update.")).toBeVisible();
  await expect(dialog.getByText("Step 2 of 3 · Repository")).toBeVisible();
  await expect(dialog.getByText("Repository published")).toHaveCount(0);
  await expect(dialog.getByRole("alert")).toHaveCount(0);
  await expect(dialog.getByRole("button", { name: "Publish", exact: true })).toBeDisabled();

  // The service state: the route itself refuses as unsupported and says the same words.
  const refused = await service
    .api("POST", `/api/projects/${projectId}/repos/${published.id}/publish`, { name: "orbyn-dev/publish-me", visibility: "private" })
    .catch((error: unknown) => String(error));
  expect(refused).toContain("422");
  expect(refused).toContain("Publishing a repository arrives in a later update.");
  expect((await rowNamed("publish-me"))?.trusted).toBe(true);
});

test("a narrow window stacks each repository row into one column with no sideways scroll", async ({ page }) => {
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section).toBeVisible();
  const first = section.getByRole("listitem").first();
  await expect(first).toBeVisible();

  const columns = () => first.evaluate((element) => getComputedStyle(element).gridTemplateColumns.trim().split(/\s+/).length);
  const boxes = () => first.evaluate((element) => {
    const box = (selector: string) => element.querySelector(selector)!.getBoundingClientRect();
    return { name: box(".repos-name"), path: box(".repos-path"), branch: box(".repos-branch"), state: box(".repos-state") };
  });

  // Wide: the row's four columns share one line.
  expect(await columns()).toBe(4);
  const wide = await boxes();
  for (const part of [wide.path, wide.branch, wide.state]) expect(Math.abs(part.top - wide.name.top)).toBeLessThan(24);

  // Narrow: one column, the parts stack name, folder, branch, state, and nothing scrolls sideways.
  await page.setViewportSize(NARROW);
  await expect.poll(columns).toBe(1);
  const stacked = await boxes();
  expect(stacked.path.top).toBeGreaterThan(stacked.name.bottom - 1);
  expect(stacked.branch.top).toBeGreaterThan(stacked.path.bottom - 1);
  expect(stacked.state.top).toBeGreaterThan(stacked.branch.bottom - 1);
  const sideScroll = await section.evaluate((element) => element.scrollWidth - element.clientWidth);
  expect(sideScroll).toBeLessThanOrEqual(0);
});

test("the Repos chip opens one row per repository as a phone sheet", async ({ page }) => {
  await page.setViewportSize(PHONE);
  await page.goto(`${service.page}#/threads/${threadId}`);
  const chip = page.getByRole("button", { name: "Repos 2", exact: true });
  await expect(chip).toBeVisible({ timeout: 30_000 });
  await chip.click();

  // A phone gets the bottom sheet: the same dialog in sheet form, with its handle, scrim and Close, not the desktop popover.
  const sheet = page.getByRole("dialog", { name: "Repositories" });
  await expect(sheet).toBeVisible();
  expect(await sheet.evaluate((element) => element.className)).toContain("pulls-sheet");
  await expect(sheet).toHaveAttribute("aria-modal", "true");
  await expect(sheet.locator("[data-sheet-handle]")).toBeVisible();
  await expect(sheet.getByRole("button", { name: "Close" })).toBeVisible();
  await expect(page.locator(".pulls-scrim")).toBeVisible();
  const task = sheet.getByLabel("Task folder", { exact: true });
  await expect(task.locator(".repos-path")).toHaveText(threadFolder.root);
  await expect(task).toContainText(`One branch for every repository: ${threadFolder.branch}`);
  await expect(sheet.locator(".repos-pop-row")).toHaveCount(2);
  for (const repository of threadFolder.repos) {
    await expect(sheet.locator(".repos-pop-row").filter({ hasText: repository.name }).locator(".repos-branch")).toHaveText(threadFolder.branch);
  }

  // The same chip on a wide window is the popover, not the sheet: no sheet class, no modal, no Close.
  await sheet.getByRole("button", { name: "Close" }).click();
  await expect(page.getByRole("dialog", { name: "Repositories" })).toHaveCount(0);
  await page.setViewportSize(WIDE);
  await chip.click();
  const popover = page.getByRole("dialog", { name: "Repositories" });
  await expect(popover).toBeVisible();
  expect(await popover.evaluate((element) => element.className)).not.toContain("pulls-sheet");
  await expect(popover).not.toHaveAttribute("aria-modal", "true");
  await expect(popover.getByRole("button", { name: "Close" })).toHaveCount(0);
  await expect(popover.locator(".repos-pop-row")).toHaveCount(2);
});

test("Clone a repository clones a real local remote over the ssh the service runs", async ({ page }) => {
  const source = sshSource("clone.test");
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section).toBeVisible();
  await section.getByRole("button", { name: "Add repository" }).click();
  await page.getByRole("menuitem", { name: "Clone a repository…" }).click();
  const dialog = page.getByRole("dialog", { name: "Clone a repository" });
  await dialog.getByRole("textbox", { name: "Git URL" }).fill(source);
  await dialog.getByRole("button", { name: "Clone", exact: true }).click();

  // The clone runs in the background into a folder the service names; wait for the service's own row to finish cloning.
  await expect.poll(async () => (await rowNamed("clone-remote"))?.clone, { timeout: 30_000 }).toBeNull();
  const cloned = await rowNamed("clone-remote");
  if (!cloned) throw new Error("The service did not list the clone.");
  const view = await repos();
  expect(dirname(cloned.path)).toBe(view.clone_directory);
  // A clone starts untrusted (docs/multi-repo.md): Git and a branch, no trust, and a remote to publish or push to.
  expect([cloned.is_git, cloned.trusted, cloned.has_remote, cloned.branch]).toEqual([true, false, true, "main"]);
  await expect(row(page, "clone-remote").getByText("Not trusted", { exact: true })).toBeVisible();

  // The bytes on disk are the remote's: its commit, its file and the source as origin.
  expect(existsSync(cloned.path)).toBe(true);
  expect(execFileSync("git", ["remote", "get-url", "origin"], { cwd: cloned.path }).toString().trim()).toBe(source);
  expect(execFileSync("git", ["rev-parse", "HEAD"], { cwd: cloned.path }).toString().trim()).toBe(
    execFileSync("git", ["rev-parse", "main"], { cwd: remoteBare }).toString().trim(),
  );
  expect(existsSync(join(cloned.path, "README.md"))).toBe(true);
});

test("Cancel stops a running clone: it stays listed as failed and its partial folder is gone", async ({ page }) => {
  const source = sshSource("stalled.test");
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section).toBeVisible();
  await section.getByRole("button", { name: "Add repository" }).click();
  await page.getByRole("menuitem", { name: "Clone a repository…" }).click();
  const dialog = page.getByRole("dialog", { name: "Clone a repository" });
  await dialog.getByRole("textbox", { name: "Git URL" }).fill(source);
  await dialog.getByRole("button", { name: "Clone", exact: true }).click();

  // The source never answers, so the row stays Cloning and offers Cancel; the confirm stops it.
  const cloning = row(page, "clone-remote-2");
  await expect(cloning.getByText("Cloning", { exact: true })).toBeVisible();
  await cloning.getByRole("button", { name: "Cancel", exact: true }).click();
  const cancel = page.getByRole("dialog", { name: "Cancel cloning clone-remote-2?" });
  await cancel.getByRole("button", { name: "Cancel clone" }).click();
  await expect(cloning.getByText("Clone failed. Cancelled.", { exact: true })).toBeVisible();
  await expect(cloning.getByRole("button", { name: "Trust…" })).toHaveCount(0);

  // The service keeps the row as failed with the service's word, removes the partial folder, and never trusts it.
  const failed = await rowNamed("clone-remote-2");
  if (!failed) throw new Error("The service did not keep the cancelled clone listed.");
  expect(failed.clone?.state).toBe("failed");
  expect(failed.clone?.message).toBe("Cancelled.");
  expect([failed.is_git, failed.trusted]).toEqual([false, false]);
  expect(existsSync(failed.path)).toBe(false);
});
