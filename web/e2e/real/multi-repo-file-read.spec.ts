import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { startProject, type Service } from "./service";

/**
 * F70 / #1001: a multi-repository thread's whole-file editor reads ANOTHER repository's file through the real service.
 *
 * The web app opens a file of a repository that is not the primary one as that repository's file tab, and the read is
 * `GET /api/threads/{id}/files/content?path=&repo_id=`. Until now that was checked against the mock only. This runs a real
 * `orbynd` with ORBYN_MULTI_REPO=1, a thread in two repositories whose same-named file differs, and proves the route-level
 * selection (the file tree and the content read) and the editor's own read, with its `repo_id`, on the page.
 */

test.setTimeout(60_000);

const PRIMARY_TEXT = "# Primary checkout\n\nOnly the primary repository says this.\n";
const SECOND_TEXT = "# Second checkout\n\nOnly the second repository says this.\n";

interface RepoRow {
  id: string;
  name: string;
  path: string;
  trusted: boolean;
}
interface TaskFolder {
  root: string;
  branch: string;
  repos: { repo_id: string; name: string }[];
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

let service: Service;
let projectId = "";
let secondRepo = "";
let secondRepoId = "";
let threadId = "";
let primaryName = "";
let secondName = "";
let primaryWorktree = "";
let secondWorktree = "";

/** A real Git repository with one commit on main, beside the fixture project, with the same-named file the primary also has. */
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

/** The id a file tab of another repository is saved under (`web/src/product/state/ui/fileTab.ts`): ["t", path, repo_id]. */
const repoFileTabId = (path: string, repoId: string) => `file:${encodeURIComponent(JSON.stringify(["t", path, repoId]))}`;

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
  // The same-named file is in both repositories before the thread is cut, so both worktrees start with it and the person's
  // change is a modification in each. The primary's copy has to be committed, since the fixture project is already a repository.
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
  const primary = folder.repos.find((repo) => repo.repo_id === projectId);
  const second = folder.repos.find((repo) => repo.repo_id === secondRepoId);
  if (!primary || !second) throw new Error("The task folder is missing a repository.");
  primaryName = primary.name;
  secondName = second.name;
  primaryWorktree = join(folder.root, primaryName);
  secondWorktree = join(folder.root, secondName);

  // The same file now says different things in the two checkouts, and each has one file the other does not, so a read of the
  // wrong repository cannot pass by accident.
  writeFileSync(join(primaryWorktree, "notes.txt"), PRIMARY_TEXT);
  writeFileSync(join(secondWorktree, "notes.txt"), SECOND_TEXT);
  writeFileSync(join(primaryWorktree, "primary-only.txt"), "primary\n");
  writeFileSync(join(secondWorktree, "second-only.txt"), "second\n");
});
test.afterAll(() => service.stop());

test("the file tree and the content read take repo_id and answer that repository's bytes", async () => {
  const tree = (repoId?: string) =>
    service.api<FileEntry[]>("GET", `/api/threads/${threadId}/files${repoId === undefined ? "" : `?repo_id=${encodeURIComponent(repoId)}`}`);
  const paths = async (repoId?: string) => (await tree(repoId)).map((entry) => entry.path);

  // ?repo_id= lists the second repository's checkout, and leaving it out lists the primary's: not the same folder.
  const secondPaths = await paths(secondRepoId);
  expect(secondPaths).toContain("second-only.txt");
  expect(secondPaths).not.toContain("primary-only.txt");
  const primaryPaths = await paths();
  expect(primaryPaths).toContain("primary-only.txt");
  expect(primaryPaths).not.toContain("second-only.txt");

  // The read route answers the file of the repository named, and a plain read answers the primary's different file.
  const read = (repoId?: string) =>
    service.api<FileContent>("GET", `/api/threads/${threadId}/files/content?path=notes.txt${repoId === undefined ? "" : `&repo_id=${encodeURIComponent(repoId)}`}`);
  expect((await read(secondRepoId)).content).toBe(SECOND_TEXT);
  expect((await read()).content).toBe(PRIMARY_TEXT);

  // A repo_id that is not one of the thread's repositories is refused, so the parameter selects a repository rather than being ignored.
  const refused = await read("not-a-repository").catch((error: unknown) => String(error));
  expect(refused).toContain("404");
});

test("the whole-file editor of another repository shows that repository's file", async ({ page }) => {
  const tab = repoFileTabId("notes.txt", secondRepoId);
  // Changes -> a repository's changed file -> "Open in editor" is the direct way in, but it is blocked by #1562: the thread's diff
  // ignores repo_id and reads the task folder, which is not a Git repository. So seed the panel tab the app remembers for a thread:
  // it reopens the whole-file editor on the second repository's file and reads it afresh, as a file tab opened earlier restores.
  // When #1562 is fixed this test can open the file from Changes instead.
  await page.addInitScript(
    (prefs: { layoutKey: string; tab: string }) => {
      localStorage.setItem(
        "orbyn.prefs.v1",
        JSON.stringify({
          panelOpen: true,
          layoutKey: prefs.layoutKey,
          panelTab: prefs.tab,
          panelTabOrder: [prefs.tab],
          layouts: { [prefs.layoutKey]: { panelTab: prefs.tab, panelTabOrder: [prefs.tab], panelTabsClosed: [], terminalOpen: false, panelMaximized: false, panelWidth: null, sideChat: null } },
        }),
      );
    },
    { layoutKey: threadId, tab },
  );
  // The read the restored editor makes is the whole-file viewer's own: the file of the repository the tab names.
  const read = page.waitForResponse((response) => new URL(response.url()).pathname === `/api/threads/${threadId}/files/content`);
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto(`${service.page}#/threads/${threadId}`);
  const answer = await read;
  const url = new URL(answer.url());
  expect(url.searchParams.get("path")).toBe("notes.txt");
  expect(url.searchParams.get("repo_id")).toBe(secondRepoId);
  expect(answer.status()).toBe(200);

  // The tab says which repository it belongs to, and the editor holds that repository's text, not the primary's.
  await expect(page.getByRole("tab", { name: `notes.txt (${secondName})`, exact: true })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Edit notes.txt" })).toHaveValue(SECOND_TEXT);
});
