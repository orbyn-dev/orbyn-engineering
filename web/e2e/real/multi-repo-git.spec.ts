import { expect, test, type Locator, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { openDetails, openPanelTab, startProject, type Service } from "./service";

/**
 * F70 / #1052: the repository choice in the thread's Git menu and in the branch and commit pickers, run against a real
 * `orbynd` with `ORBYN_MULTI_REPO=1`.
 *
 * The thread works in two repositories whose histories and branches differ, so a read that went to the wrong repository
 * could not pass: the primary's commits hold PRIMARY_ONLY and the second repository's hold SECOND_ONLY, and a branch
 * SECOND_BRANCH exists in the second alone. The spec proves what the UI sends — `repo_id` on the reads of the picked
 * repository and left out for the primary — and that a branch created from the dialog lands in that repository's worktree
 * and nowhere else. The mock cannot show any of this: it is the wire that is being checked.
 */

test.setTimeout(90_000);

const WINDOW = { width: 1500, height: 900 };

interface RepoRow {
  id: string;
  name: string;
  path: string;
  trusted: boolean;
}
interface TaskFolder {
  root: string;
  branch: string;
  repos: { repo_id: string; name: string; branch: string }[];
}
interface BranchInfo {
  name: string;
  current: boolean;
  remote: boolean;
}
interface CommitInfo {
  sha: string;
  subject: string;
}

let service: Service;
let projectId = "";
let secondRepoId = "";
let threadId = "";
let threadBranch = "";
let primaryName = "";
let secondName = "";
let primaryWorktree = "";
let secondWorktree = "";

/** A commit subject only the primary repository's history holds. */
const PRIMARY_ONLY = "Primary only";
/** A commit subject only the second repository's history holds. */
const SECOND_ONLY = "Second only";
/** A branch only the second repository has, cut before the thread exists, so the branch list drawn is that repository's. */
const SECOND_BRANCH = "second-only";

const git = (cwd: string, ...args: string[]) => execFileSync("git", args, { cwd, encoding: "utf8" }).trim();

/** Writes one file, stages it and commits it, so each repository's history is its own. */
function commit(path: string, message: string, file: string, text: string) {
  writeFileSync(join(path, file), text);
  git(path, "add", file);
  git(path, "-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", message);
}

/** A real Git repository with its own two commits on main, one of which no other repository sees. */
function makeRepo(parent: string, name: string): string {
  const path = join(parent, name);
  mkdirSync(path);
  git(path, "init", "-q", "-b", "main");
  commit(path, "First", "README.md", `# ${name}\n`);
  commit(path, SECOND_ONLY, "second.txt", "Only the second repository's history has this commit.\n");
  return path;
}

/** Registers and trusts one repository of the project through the real routes. */
async function trustRepository(path: string): Promise<RepoRow> {
  const view = await service.api<{ repositories: RepoRow[] }>("GET", `/api/projects/${projectId}/repos`);
  const found = view.repositories.find((entry) => entry.path === path);
  const repository = found ?? (await service.api<RepoRow>("POST", `/api/projects/${projectId}/repos/add-folder`, { path }));
  if (!repository.trusted) await service.api("POST", `/api/projects/${projectId}/repos/${repository.id}/trust`);
  return repository;
}

const branchesOf = (repoId?: string) =>
  service.api<BranchInfo[]>("GET", `/api/threads/${threadId}/branches${repoId === undefined ? "" : `?repo_id=${encodeURIComponent(repoId)}`}`);
const commitsOf = (repoId?: string) =>
  service.api<CommitInfo[]>("GET", `/api/threads/${threadId}/commits${repoId === undefined ? "" : `?repo_id=${encodeURIComponent(repoId)}`}`);

/** The repository chip the Changes panel and the branch dialog draw, whose name is the repository it would act on. */
const repositoryChip = (where: Page | Locator, name: string) =>
  where.getByRole("button", { name: `Repository: ${name}`, exact: true });

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ multiRepo: true, fakeEngines: true }));
  // The primary's own commit, made before the thread is cut so its worktree's history holds it.
  commit(service.repo, PRIMARY_ONLY, "primary.txt", "Only the primary repository's history has this commit.\n");
  const secondRepo = makeRepo(dirname(service.dataDir), "second-repo");
  secondRepoId = (await trustRepository(secondRepo)).id;
  git(secondRepo, "branch", SECOND_BRANCH);

  // One thread in both repositories: one task folder and one worktree per repository on the thread's branch.
  const thread = await service.api<{ id: string }>("POST", `/api/projects/${projectId}/repo-threads`, {
    repos: [{ repo_id: projectId, base_branch: "main" }, { repo_id: secondRepoId, base_branch: "main" }],
    engine: "codex",
    model: null,
    permission_mode: "supervised",
  });
  threadId = thread.id;
  const folder = await service.api<TaskFolder>("GET", `/api/threads/${threadId}/task-folder`);
  threadBranch = folder.branch;
  const primary = folder.repos.find((repo) => repo.repo_id === projectId);
  const second = folder.repos.find((repo) => repo.repo_id === secondRepoId);
  if (!primary || !second) throw new Error("The task folder is missing a repository.");
  primaryName = primary.name;
  secondName = second.name;
  primaryWorktree = join(folder.root, primaryName);
  secondWorktree = join(folder.root, secondName);
});
test.afterAll(() => service.stop());

test("the thread's Git reads answer one repository at a time, and leaving repo_id out is the primary", async () => {
  // Branches: the second repository's own branch is in its list and not the primary's; its own id reads as no id does.
  const primaryBranches = await branchesOf();
  const secondBranches = await branchesOf(secondRepoId);
  expect(primaryBranches.some((branch) => branch.name === SECOND_BRANCH)).toBe(false);
  expect(secondBranches.some((branch) => branch.name === SECOND_BRANCH)).toBe(true);
  expect(primaryBranches.some((branch) => branch.current && branch.name === threadBranch)).toBe(true);
  expect(await branchesOf(projectId)).toEqual(primaryBranches);
  expect(await branchesOf("not-a-repository").catch((error: unknown) => String(error))).toContain("404");

  // Commits: each repository's history holds its own commit and not the other's.
  const primaryCommits = (await commitsOf()).map((commit) => commit.subject);
  const secondCommits = (await commitsOf(secondRepoId)).map((commit) => commit.subject);
  expect(primaryCommits).toContain(PRIMARY_ONLY);
  expect(primaryCommits).not.toContain(SECOND_ONLY);
  expect(secondCommits).toContain(SECOND_ONLY);
  expect(secondCommits).not.toContain(PRIMARY_ONLY);
});

test("the commit picker and the header Git menu follow the repository picked in Changes", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  await page.goto(`${service.page}#/threads/${threadId}`);
  const panel = await openPanelTab(page, "Diff");

  // The Commit scope opens the commit list of the repository the pickers are on, the primary to begin with.
  const primaryRead = page.waitForResponse((response) => {
    const url = new URL(response.request().url());
    return response.request().method() === "GET" && url.pathname === `/api/threads/${threadId}/commits`;
  });
  await panel.getByRole("button", { name: "Scope: Working tree" }).click();
  await page.getByRole("menu", { name: "Scope" }).getByRole("menuitem", { name: "Commit", exact: true }).click();
  // The primary's calls leave repo_id out, which is what the service's default answers.
  expect(new URL((await primaryRead).url()).searchParams.get("repo_id")).toBeNull();
  const picker = page.getByRole("menu", { name: "Commit" });
  await expect(picker).toBeVisible();
  await expect(picker.locator(".menu-heading")).toHaveText(`Commits on ${threadBranch}`);
  await expect(picker.getByRole("menuitem", { name: new RegExp(PRIMARY_ONLY) })).toBeVisible();
  await expect(picker.getByRole("menuitem", { name: new RegExp(SECOND_ONLY) })).toHaveCount(0);

  // The Changes header names the repository the two scopes act on: the project's own, the primary.
  await page.keyboard.press("Escape");
  await expect(picker).toHaveCount(0);
  await expect(repositoryChip(page.locator(".repos-scope"), primaryName)).toBeVisible();

  // Picking the other repository re-reads that repository's commits, with its repo_id on the query.
  const secondRead = page.waitForResponse((response) => {
    const url = new URL(response.request().url());
    return response.request().method() === "GET" && url.pathname === `/api/threads/${threadId}/commits` && url.searchParams.get("repo_id") === secondRepoId;
  });
  await repositoryChip(page.locator(".repos-scope"), primaryName).click();
  await page.getByRole("menu", { name: "Repository" }).getByRole("menuitem", { name: secondName, exact: true }).click();
  expect((await secondRead).ok()).toBe(true);
  await expect(repositoryChip(page.locator(".repos-scope"), secondName)).toBeVisible();

  // The picker now lists THAT repository's commits, and the primary's are gone.
  await page.getByRole("button", { name: /^Compare / }).click();
  const secondPicker = page.getByRole("menu", { name: "Commit" });
  await expect(secondPicker).toBeVisible();
  await expect(secondPicker.getByRole("menuitem", { name: new RegExp(SECOND_ONLY) })).toBeVisible();
  await expect(secondPicker.getByRole("menuitem", { name: new RegExp(PRIMARY_ONLY) })).toHaveCount(0);

  // The header's Git menu acts on the same repository — one choice shared by the pickers and the menu — and Publish,
  // which belongs to the project's primary repository, is off with the reason as its title.
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Git", exact: true }).click();
  const gitMenu = page.getByRole("menu", { name: "Git actions" });
  await expect(gitMenu.locator(".menu-heading")).toHaveText(secondName);
  const publish = gitMenu.getByRole("menuitem", { name: "Publish repository", exact: true });
  await expect(publish).toHaveAttribute("aria-disabled", "true");
  await expect(publish).toHaveAttribute("title", "Publishing applies to the project's primary repository.");
  // Committing in that repository is still offered: only publishing is the primary's own.
  const commit = gitMenu.getByRole("menuitem", { name: "Commit", exact: true });
  await expect(commit).not.toHaveAttribute("aria-disabled", "true");

  // Back to the primary: the menu names it and Publish is on again.
  await page.keyboard.press("Escape");
  await repositoryChip(page.locator(".repos-scope"), secondName).click();
  await page.getByRole("menu", { name: "Repository" }).getByRole("menuitem", { name: primaryName, exact: true }).click();
  await page.getByRole("button", { name: "Git", exact: true }).click();
  const primaryMenu = page.getByRole("menu", { name: "Git actions" });
  await expect(primaryMenu.locator(".menu-heading")).toHaveText(primaryName);
  await expect(primaryMenu.getByRole("menuitem", { name: "Publish repository", exact: true })).not.toHaveAttribute("aria-disabled", "true");
});

test("the branch dialog reads, creates and switches in the chosen repository", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  await page.goto(`${service.page}#/threads/${threadId}`);
  const details = await openDetails(page);
  // The row names the chosen repository's branch; the primary to begin with.
  const row = details.getByRole("button", { name: `Branch ${threadBranch}`, exact: true });

  // Opening the dialog for the primary reads the branches with no repo_id on the query.
  const primaryRead = page.waitForResponse((response) => {
    const url = new URL(response.request().url());
    return response.request().method() === "GET" && url.pathname === `/api/threads/${threadId}/branches`;
  });
  await row.click();
  const dialog = page.getByRole("dialog", { name: "Switch branch" });
  await expect(dialog).toBeVisible();
  expect(new URL((await primaryRead).url()).searchParams.get("repo_id")).toBeNull();
  await expect(repositoryChip(dialog, primaryName)).toBeVisible();

  // Choosing the second repository in the dialog reads ITS branches: the one only it has is listed.
  const secondRead = page.waitForResponse((response) => {
    const url = new URL(response.request().url());
    return response.request().method() === "GET" && url.pathname === `/api/threads/${threadId}/branches` && url.searchParams.get("repo_id") === secondRepoId;
  });
  await repositoryChip(dialog, primaryName).click();
  await page.getByRole("menu", { name: "Repository" }).getByRole("menuitem", { name: secondName, exact: true }).click();
  expect((await secondRead).ok()).toBe(true);
  await expect(repositoryChip(dialog, secondName)).toBeVisible();
  await expect(dialog.getByRole("list", { name: "Branches" }).getByRole("button", { name: SECOND_BRANCH, exact: true })).toBeVisible();

  // Creating a branch is one write naming the repository: the branch lands in its worktree, and the other stays put.
  const liveBranch = `picker-${randomUUID().slice(0, 8)}`;
  const created = page.waitForRequest((request) => request.method() === "POST" && new URL(request.url()).pathname === `/api/threads/${threadId}/branches`);
  await dialog.getByLabel("Search branches").fill(liveBranch);
  await dialog.getByRole("button", { name: new RegExp(`^Create branch “${liveBranch}”`) }).click();
  expect((await created).postDataJSON()).toEqual({ name: liveBranch, repo_id: secondRepoId });
  await expect(dialog).toHaveCount(0);

  await expect.poll(() => git(secondWorktree, "rev-parse", "--abbrev-ref", "HEAD")).toBe(liveBranch);
  expect(git(primaryWorktree, "rev-parse", "--abbrev-ref", "HEAD")).toBe(threadBranch);
  const secondBranches = await branchesOf(secondRepoId);
  expect(secondBranches.some((branch) => branch.current && branch.name === liveBranch)).toBe(true);
  expect((await branchesOf()).some((branch) => branch.name === liveBranch)).toBe(false);

  // The details row is the chosen repository's: the thread's branch, with that repository named under it. Switching the
  // checkout's branch is that worktree's own and leaves the thread's recorded branch alone.
  const branchRow = details.getByRole("button", { name: `Branch ${threadBranch}`, exact: true });
  await expect(branchRow).toBeVisible();
  await expect(branchRow).toContainText(secondName);
});

test("the Git menu commits in the chosen repository only", async ({ page }) => {
  await page.setViewportSize(WINDOW);
  await page.goto(`${service.page}#/threads/${threadId}`);
  const panel = await openPanelTab(page, "Diff");

  // The pickers' scope control is where a multi-repository thread picks the repository, and the header menu shares it.
  await panel.getByRole("button", { name: "Scope: Working tree" }).click();
  await page.getByRole("menu", { name: "Scope" }).getByRole("menuitem", { name: "Commit", exact: true }).click();
  await repositoryChip(page.locator(".repos-scope"), primaryName).click();
  await page.getByRole("menu", { name: "Repository" }).getByRole("menuitem", { name: secondName, exact: true }).click();
  await expect(repositoryChip(page.locator(".repos-scope"), secondName)).toBeVisible();
  await page.keyboard.press("Escape");

  // A change of the second repository's own for the menu to commit, and one pending in the primary that the commit must
  // leave alone: with the primary clean, an implementation that also staged it there could pass every assertion.
  writeFileSync(join(secondWorktree, "picker.txt"), "Committed through the Git menu of the chosen repository.\n");
  writeFileSync(join(primaryWorktree, "primary-only.txt"), "Another repository's commit neither stages nor commits this.\n");
  const primaryHead = git(primaryWorktree, "rev-parse", "HEAD");
  const primaryStatus = git(primaryWorktree, "status", "--porcelain");
  const subject = `From the picker ${randomUUID().slice(0, 6)}`;
  await page.getByRole("button", { name: "Git", exact: true }).click();
  await page.getByRole("menuitem", { name: "Commit", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Commit" });
  // The dialog says which repository it would act on, and the write names it.
  await expect(dialog).toContainText(`Acts on ${secondName} only.`);
  const sent = page.waitForRequest((request) => request.method() === "POST" && new URL(request.url()).pathname === `/api/threads/${threadId}/git/action`);
  await dialog.getByLabel("Commit message").fill(subject);
  await dialog.getByRole("button", { name: "Commit", exact: true }).click();
  expect(((await sent).postDataJSON() as { repo_id?: string }).repo_id).toBe(secondRepoId);
  await expect(dialog.getByRole("list", { name: "Result" }).getByRole("img", { name: "Done" })).toBeVisible();

  // The commit is that repository's: its worktree holds it, and the primary's history does not.
  expect(git(secondWorktree, "log", "-1", "--format=%s")).toBe(subject);
  expect(git(primaryWorktree, "log", "-1", "--format=%s")).not.toBe(subject);
  // The primary's own pending change is still pending: the write neither staged it nor committed it.
  expect(git(primaryWorktree, "rev-parse", "HEAD")).toBe(primaryHead);
  expect(git(primaryWorktree, "status", "--porcelain")).toBe(primaryStatus);
});
