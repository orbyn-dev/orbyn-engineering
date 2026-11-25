import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { openPanelTab, startProject, type Service } from "./service";

/**
 * F70 / #1570: a multi-repository thread's Changes panel shows each repository's OWN diff through the real service.
 *
 * The Changes read is `GET /api/threads/{id}/diff?repo_id=`, one repository per read (#1568): the panel reads once per trusted
 * repository and groups the files by the repository each came from. This runs a real `orbynd` with ORBYN_MULTI_REPO=1 and a
 * thread in two repositories whose same-named file says something different and has a different number of changed lines in each.
 * It opens Changes, picks the second repository's changed file and checks that file's patch, list row and summary are that
 * repository's, then the primary's different ones. The `files?repo_id=` file-read evidence (#1001/#1562) is asserted too, so the
 * file routing keeps passing beside the diff routing this spec covers.
 */

test.setTimeout(60_000);

/** Working-tree copies, cut from each repository's committed base ("# primary base" / "# second base"), so both are modifications. */
const PRIMARY_TEXT = "# Primary checkout\n\nOnly the primary repository says this.\n";
const SECOND_TEXT = "# Second checkout\n\nThis is the second repository's own change.\nAnd it has a second line.\n";
/** `git diff main` counts of the two edits above: different, so a summary cannot pass by naming the other repository's change. */
const PRIMARY_COUNTS = "+3 −1";
const SECOND_COUNTS = "+4 −1";

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
interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  ignored: boolean;
}
interface FileContent {
  content: string;
  revision: string;
}
interface DiffFile {
  path: string;
  status: string;
  additions: number;
  deletions: number;
  patch: string;
  repo_id?: string;
}

let service: Service;
let projectId = "";
let secondRepo = "";
let secondRepoId = "";
let threadId = "";
let primaryName = "";
let secondName = "";
let primaryWorktree = "";
let secondWorktree = "";
let threadBranch = "";

/** A real Git repository with one commit on main, beside the fixture project, holding the same-named file the primary also has. */
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

const git = (cwd: string, ...args: string[]) => execFileSync("git", args, { cwd, stdio: "ignore" });

/** Registers and trusts one repository of the project through the real routes. */
async function trustRepository(path: string): Promise<RepoRow> {
  const view = await service.api<{ repositories: RepoRow[] }>("GET", `/api/projects/${projectId}/repos`);
  const found = view.repositories.find((entry) => entry.path === path);
  const repository = found ?? (await service.api<RepoRow>("POST", `/api/projects/${projectId}/repos/add-folder`, { path }));
  if (!repository.trusted) await service.api("POST", `/api/projects/${projectId}/repos/${repository.id}/trust`);
  return repository;
}

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ multiRepo: true, fakeEngines: true }));
  // The same-named file is in both repositories before the thread is cut, so each worktree starts with it and the person's change
  // is a modification in each. The primary's copy is committed, since the fixture project is already a repository.
  writeFileSync(join(service.repo, "notes.txt"), "# primary base\n");
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
  threadBranch = folder.branch;
  const primary = folder.repos.find((repo) => repo.repo_id === projectId);
  const second = folder.repos.find((repo) => repo.repo_id === secondRepoId);
  if (!primary || !second) throw new Error("The task folder is missing a repository.");
  primaryName = primary.name;
  secondName = second.name;
  primaryWorktree = join(folder.root, primaryName);
  secondWorktree = join(folder.root, secondName);

  // The same file now says something different in each checkout, with a different number of added lines.
  writeFileSync(join(primaryWorktree, "notes.txt"), PRIMARY_TEXT);
  writeFileSync(join(secondWorktree, "notes.txt"), SECOND_TEXT);
});
test.afterAll(() => service.stop());

const query = (repoId: string | undefined) => (repoId === undefined ? "" : `&repo_id=${encodeURIComponent(repoId)}`);

test("the diff read answers one repository at a time, and the file reads keep taking repo_id", async () => {
  const diff = (repoId?: string) =>
    service.api<DiffFile[]>("GET", `/api/threads/${threadId}/diff?scope=worktree${query(repoId)}`);

  // No repo_id is the primary repository; its own id is the same read, unlike the second repository's.
  const primary = await diff();
  const second = await diff(secondRepoId);
  expect(primary.map((file) => [file.path, file.repo_id])).toEqual([["notes.txt", projectId]]);
  expect(second.map((file) => [file.path, file.repo_id])).toEqual([["notes.txt", secondRepoId]]);
  expect(primary[0]?.patch).toContain("Only the primary repository says this.");
  expect(primary[0]?.patch).not.toContain("second repository");
  expect(second[0]?.patch).toContain("This is the second repository's own change.");
  expect(second[0]?.patch).not.toContain("primary repository");
  // The summary differs too, so a row or header that shows the other repository's counts is caught.
  expect([primary[0]?.additions, primary[0]?.deletions]).toEqual([3, 1]);
  expect([second[0]?.additions, second[0]?.deletions]).toEqual([4, 1]);
  expect(await diff(projectId)).toEqual(primary);

  // A repository that is not the thread's is refused, so the parameter selects a repository rather than being ignored.
  const refusedDiff = await diff("not-a-repository").catch((error: unknown) => String(error));
  expect(refusedDiff).toContain("404");

  // The existing file-read evidence (#1001/#1562) still passes: the tree and the content read take repo_id and answer that
  // repository's bytes, and the primary is what leaving repo_id out means.
  const paths = async (repoId?: string) =>
    (await service.api<FileEntry[]>("GET", `/api/threads/${threadId}/files${repoId === undefined ? "" : `?repo_id=${encodeURIComponent(repoId)}`}`)).map((entry) => entry.path);
  const content = (repoId?: string) =>
    service.api<FileContent>("GET", `/api/threads/${threadId}/files/content?path=notes.txt${query(repoId)}`);
  expect(await paths(secondRepoId)).toContain("notes.txt");
  expect(await paths()).toContain("notes.txt");
  expect((await content(secondRepoId)).content).toBe(SECOND_TEXT);
  expect((await content(projectId)).content).toBe(PRIMARY_TEXT);
  expect((await content()).content).toBe(PRIMARY_TEXT);
  const refusedRead = await content("not-a-repository").catch((error: unknown) => String(error));
  expect(refusedRead).toContain("404");
});

test("Changes groups the two repositories and shows the picked repository's own diff", async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto(`${service.page}#/threads/${threadId}`);
  const panel = await openPanelTab(page, "Diff");
  const list = panel.getByRole("group", { name: "Changed files" });
  // One group per repository, in the thread's order, named with that repository's name.
  const primaryGroup = list.getByRole("group", { name: `${primaryName} changes` });
  const secondGroup = list.getByRole("group", { name: `${secondName} changes` });
  await expect(primaryGroup).toBeVisible();
  await expect(secondGroup).toBeVisible();
  // Each group's header names the thread's branch and sums up that repository's own change.
  await expect(primaryGroup.locator(".fl-repo-head")).toContainText(primaryName);
  await expect(secondGroup.locator(".fl-repo-head")).toContainText(secondName);
  for (const group of [primaryGroup, secondGroup]) await expect(group.locator(".fl-repo-head")).toContainText(threadBranch);
  await expect(primaryGroup.locator(".fl-repo-head .num")).toHaveText(PRIMARY_COUNTS);
  await expect(secondGroup.locator(".fl-repo-head .num")).toHaveText(SECOND_COUNTS);

  // Each group has its own row for the same-named changed file, and the rows do not share one summary.
  const row = (group: typeof primaryGroup) => group.getByRole("button", { name: /^Modified notes\.txt/ });
  const primaryRow = row(primaryGroup);
  const secondRow = row(secondGroup);
  await expect(primaryRow).toHaveCount(1);
  await expect(secondRow).toHaveCount(1);
  await expect(primaryRow.locator(".num")).toHaveText(PRIMARY_COUNTS);
  await expect(secondRow.locator(".num")).toHaveText(SECOND_COUNTS);

  // The file on show starts as the primary repository's: its header names the repository and the patch is its own.
  const view = page.locator("[data-diff-file='notes.txt']");
  await expect(view).toBeVisible();
  await expect(view.locator(".repo-pill")).toHaveText(primaryName);
  await expect(view.locator(".fh2 .num")).toHaveText(PRIMARY_COUNTS);
  // A diff line is drawn with a hidden "Added:"/"Removed:" marker, so the line is matched by its text, not exactly.
  await expect(view.getByText("Only the primary repository says this.")).toBeVisible();
  await expect(view.getByText("This is the second repository's own change.")).toHaveCount(0);

  // Picking the second repository's row shows THAT repository's diff: its name on the file line, its patch and its summary.
  await secondRow.click();
  await expect(view.locator(".repo-pill")).toHaveText(secondName);
  await expect(view.locator(".fh2 .num")).toHaveText(SECOND_COUNTS);
  await expect(view.getByText("This is the second repository's own change.")).toBeVisible();
  await expect(view.getByText("And it has a second line.")).toBeVisible();
  await expect(view.getByText("Only the primary repository says this.")).toHaveCount(0);

  // Picking the primary's row again returns to its different patch and summary.
  await primaryRow.click();
  await expect(view.locator(".repo-pill")).toHaveText(primaryName);
  await expect(view.locator(".fh2 .num")).toHaveText(PRIMARY_COUNTS);
  await expect(view.getByText("Only the primary repository says this.")).toBeVisible();
  await expect(view.getByText("This is the second repository's own change.")).toHaveCount(0);
});
