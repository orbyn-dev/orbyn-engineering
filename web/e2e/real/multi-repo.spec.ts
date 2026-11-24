import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { projectSettingsUrl, startProject, threadIdOf, type Service } from "./service";

// The second test starts from the project the first one changed: it adds and trusts the second repository there.
test.describe.configure({ mode: "serial" });

/**
 * F70: multi-repository projects and task folders, through a real `orbynd` with `ORBYN_MULTI_REPO=1`.
 *
 * A project of two local Git folders is listed in Settings with each folder, branch and trust state; a second folder is
 * added from the page and trusted. A thread started in both gets ONE task folder, one branch in each repository (checked
 * in the service's answer and on disk with Git), the header Repos chip and its popover, and a third repository added to
 * the running thread later.
 */

const WINDOW = { width: 1500, height: 1000 };

let service: Service;
let projectId = "";
let secondRepo = "";
let thirdRepo = "";

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

interface RepoRow {
  id: string;
  name: string;
  path: string;
  branch: string | null;
  trusted: boolean;
  primary: boolean;
}
interface TaskFolder {
  root: string;
  branch: string;
  repos: { repo_id: string; name: string; branch: string; trusted: boolean }[];
  max_per_thread: number;
}

const repositories = (page: Page) => page.getByRole("region", { name: "Repositories" });
const row = (page: Page, name: string) => repositories(page).getByRole("listitem").filter({ hasText: name });

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ multiRepo: true, fakeEngines: true }));
  const parent = dirname(service.dataDir);
  secondRepo = makeRepo(parent, "second-repo");
  thirdRepo = makeRepo(parent, "third-repo");
});
test.afterAll(() => service.stop());
test.beforeEach(async ({ page }) => {
  await page.setViewportSize(WINDOW);
});

test("the repositories group lists the project's folders with branch and trust, and a second is added from it", async ({ page }) => {
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section).toBeVisible();
  await expect(section.getByRole("listitem")).toHaveCount(1);
  const first = row(page, "my-repo");
  await expect(first).toContainText(service.repo);
  await expect(first.locator(".repos-branch")).toHaveText("main");
  await expect(first.getByText("Trusted", { exact: true })).toBeVisible();

  // The second folder joins in place from Add repository > Add a folder…, and starts untrusted.
  await section.getByRole("button", { name: "Add repository" }).click();
  await page.getByRole("menuitem", { name: "Add a folder…" }).click();
  const dialog = page.getByRole("dialog", { name: "Add a folder" });
  await dialog.getByLabel("Folder on this computer").fill(secondRepo);
  await dialog.getByRole("button", { name: "Add", exact: true }).click();
  await expect(section.getByRole("listitem")).toHaveCount(2);
  const second = row(page, "second-repo");
  await expect(second).toContainText(secondRepo);
  await expect(second.locator(".repos-branch")).toHaveText("main");
  await expect(second.getByText("Not trusted", { exact: true })).toBeVisible();

  // Trust is its own write, one folder at a time.
  await second.getByRole("button", { name: "Trust…" }).click();
  const trust = page.getByRole("dialog", { name: "Trust second-repo?" });
  await expect(trust).toContainText("Agents can read files, run commands and edit code in this folder.");
  await trust.getByRole("button", { name: "Trust folder" }).click();
  await expect(second.getByText("Trusted", { exact: true })).toBeVisible();

  // The service holds both repositories: names, folders, branches and trust, the project's own folder first and primary.
  const view = await service.api<{ repositories: RepoRow[]; max_per_thread: number }>("GET", `/api/projects/${projectId}/repos`);
  expect(view.repositories.map((entry) => entry.name)).toEqual(["my-repo", "second-repo"]);
  expect(view.repositories.map((entry) => entry.path)).toEqual([service.repo, secondRepo]);
  expect(view.repositories.map((entry) => entry.branch)).toEqual(["main", "main"]);
  expect(view.repositories.map((entry) => entry.trusted)).toEqual([true, true]);
  expect(view.repositories.map((entry) => entry.primary)).toEqual([true, false]);
  expect(view.max_per_thread).toBe(8);
});

test("a thread in two repositories gets one task folder and one branch in each, and gains a repository later", async ({ page }) => {
  // The new-thread screen offers the repository dialog once the project has two folders.
  await page.goto(`${service.page}#/new/${projectId}`);
  const start = page.getByRole("button", { name: "Repositories: 2", exact: true });
  await expect(start).toBeVisible({ timeout: 30_000 });
  await start.click();
  const dialog = page.getByRole("dialog").filter({ hasText: "New thread in my-repo" });
  await expect(dialog).toBeVisible();
  const picks = dialog.getByRole("list", { name: "Repositories" });
  await expect(picks.getByRole("listitem")).toHaveCount(2);
  await expect(picks.getByRole("checkbox")).toHaveCount(2);
  const boxes = picks.getByRole("checkbox");
  await expect(boxes.nth(0)).toBeChecked();
  await expect(boxes.nth(1)).toBeChecked();
  await expect(dialog.getByLabel("Branch in my-repo")).toHaveValue("main");
  await expect(dialog.getByLabel("Branch in second-repo")).toHaveValue("main");

  // Start is one write: the service makes the thread, the task folder and a worktree per repository.
  const started = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/projects/${projectId}/repo-threads`);
  await dialog.getByRole("button", { name: "Start in 2 repositories" }).click();
  const answer = await (await started).json();
  await expect(page).toHaveURL(/#\/threads\//);
  const threadId = threadIdOf(page);
  expect(answer.id).toBe(threadId);

  // The service's answer: one task folder under threads/<id>/, one branch name in every repository, two worktrees.
  const folder = await service.api<TaskFolder>("GET", `/api/threads/${threadId}/task-folder`);
  expect(folder.root).toContain(`threads/${threadId}`);
  expect(folder.max_per_thread).toBe(8);
  expect(folder.repos.map((repo) => repo.name)).toEqual(["my-repo", "second-repo"]);
  expect(folder.branch).toMatch(/^orbyn\/[0-9a-z]{8}$/);
  for (const repository of folder.repos) expect(repository.branch).toBe(folder.branch);
  for (const repository of folder.repos) {
    const worktree = join(folder.root, repository.name);
    expect(existsSync(worktree)).toBe(true);
    expect(execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], { cwd: worktree }).toString().trim()).toBe(folder.branch);
  }
  const threadRepos = await service.api<{ repo_id: string; name: string; branch: string; trusted: boolean }[]>("GET", `/api/threads/${threadId}/repos`);
  expect(threadRepos.map((repo) => [repo.name, repo.branch, repo.trusted])).toEqual(folder.repos.map((repo) => [repo.name, folder.branch, true]));

  // The header's Repos chip and its popover: the task folder as text, one branch for every repository, one row each.
  await page.getByRole("button", { name: "Repos 2", exact: true }).click();
  const popover = page.getByRole("dialog", { name: "Repositories" });
  await expect(popover).toBeVisible();
  await expect(popover).toContainText("Working in 2 repositories");
  const task = popover.locator(".repos-task");
  await expect(task).toContainText("Task folder");
  await expect(task.locator(".repos-path")).toHaveText(folder.root);
  await expect(task).toContainText(`One branch for every repository: ${folder.branch}`);
  await expect(task).toContainText("2 of 8 repositories");
  await expect(popover.locator(".repos-pop-row")).toHaveCount(2);
  for (const repository of folder.repos) {
    const line = popover.locator(".repos-pop-row").filter({ hasText: repository.name });
    await expect(line.locator(".repos-branch")).toHaveText(folder.branch);
    await expect(line.getByText("Trusted", { exact: true })).toBeVisible();
  }

  // A repository added to the project later can join the running thread: its worktree is cut on the same branch.
  const added = await service.api<RepoRow>("POST", `/api/projects/${projectId}/repos/add-folder`, { path: thirdRepo });
  await service.api("POST", `/api/projects/${projectId}/repos/${added.id}/trust`);
  // Close the popover by its own chip and wait for it to leave the DOM, so the next click opens a fresh one (the list behind
  // it is read again for the project, which now holds the third repository).
  await page.getByRole("button", { name: "Repos 2", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Repositories" })).toHaveCount(0);
  await page.getByRole("button", { name: "Repos 2", exact: true }).click();
  const again = page.getByRole("dialog", { name: "Repositories" });
  await expect(again).toBeVisible();
  await again.getByRole("button", { name: "Add a repository to this thread…" }).click();
  await page.getByRole("menuitem", { name: "third-repo" }).click();
  await expect(again.locator(".repos-pop-row")).toHaveCount(3);
  await expect(again.locator(".repos-task")).toContainText("3 of 8 repositories");
  await expect(again.locator(".repos-pop-row").filter({ hasText: "third-repo" }).locator(".repos-branch")).toHaveText(folder.branch);
  const grown = await service.api<{ name: string; branch: string }[]>("GET", `/api/threads/${threadId}/repos`);
  expect(grown.map((repo) => [repo.name, repo.branch])).toEqual([["my-repo", folder.branch], ["second-repo", folder.branch], ["third-repo", folder.branch]]);
  const thirdWorktree = join(folder.root, "third-repo");
  expect(existsSync(thirdWorktree)).toBe(true);
  expect(execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], { cwd: thirdWorktree }).toString().trim()).toBe(folder.branch);

  // The repositories list carries each repository's own branch, the thread's one.
  await page.goto(await projectSettingsUrl(service, projectId));
  const section = repositories(page);
  await expect(section.getByRole("listitem")).toHaveCount(3);
  for (const name of ["my-repo", "second-repo", "third-repo"]) {
    await expect(row(page, name).locator(".repos-branch")).toHaveText("main");
  }
});
