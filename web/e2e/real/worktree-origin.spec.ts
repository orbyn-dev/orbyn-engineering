import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startProject, type Service } from "./service";

/**
 * F28, a new worktree starts at origin's head: the project is a clone with a local bare origin; after another clone
 * pushes a new commit to origin, a worktree thread starts from that commit, not from the project's own older HEAD, and
 * the project's primary checkout stays on its own branch. The service's fetch of origin runs as part of the start, so
 * this only passes if it fetched rather than used the clone's stale remote-tracking ref.
 */

const git = (cwd: string, ...args: string[]): string => execFileSync("git", args, { cwd, encoding: "utf8" }).trim();

let service: Service;
let projectId = "";

interface Thread {
  id: string;
  worktree?: { path?: string; branch?: string } | null;
}

/** The worktree a thread runs in, from the project's own worktree list. */
function worktreeOf(path: string | undefined): string {
  const listed = git(service.repo, "worktree", "list", "--porcelain").split("\n\n").map((block) => block.split("\n")[0] ?? "").map((line) => line.replace(/^worktree /, ""));
  const found = path ? listed.find((entry) => entry === path) : undefined;
  if (!found) throw new Error(`no worktree for the thread in ${listed.join(", ")}`);
  return found;
}

test.beforeAll(async () => {
  // The fixture gives the project a real local bare origin on disk and pushes to it here; the service clones that remote as
  // the project, and fetches it when it makes the worktree in the second step.
  ({ service, projectId } = await startProject({ fakeEngines: true, forge: true }));
  // Another clone pushes a new commit to that origin: the project's own HEAD does not move.
  const holder = mkdtempSync(join(tmpdir(), "origin-head-"));
  execFileSync("git", ["clone", "-q", service.remote, holder], { stdio: "ignore" });
  writeFileSync(join(holder, "origin.txt"), "advanced origin\n");
  git(holder, "add", ".");
  git(holder, "-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "Advance origin");
  git(holder, "push", "-q", "origin", "main");
});

test.afterAll(async () => {
  await service?.stop();
});

test("a worktree thread starts at origin's advanced head, not the clone's HEAD", async () => {
  const advanced = execFileSync("git", ["rev-parse", "HEAD"], { cwd: service.remote, encoding: "utf8" }).trim();
  const checkoutHead = git(service.repo, "rev-parse", "HEAD");
  expect(advanced).not.toBe(checkoutHead);

  const thread = await service.api<Thread>("POST", "/api/threads", {
    project_id: projectId, engine: "codex", model: null, title: "From origin", worktree: "origin-head", skill_profile: null,
  });
  await expect
    .poll(async () => {
      const current = await service.api<Thread>("GET", `/api/threads/${thread.id}`);
      return current.worktree?.path ?? "";
    }, { timeout: 30_000 })
    .not.toBe("");

  const current = await service.api<Thread>("GET", `/api/threads/${thread.id}`);
  const worktree = worktreeOf(current.worktree?.path);
  expect(git(worktree, "rev-parse", "HEAD")).toBe(advanced);
  expect(git(worktree, "rev-parse", "--abbrev-ref", "HEAD")).not.toBe("main");
  // The project's primary checkout did not move: a worktree start never rewinds or re-branches it.
  expect(git(service.repo, "rev-parse", "HEAD")).toBe(checkoutHead);
  expect(git(service.repo, "rev-parse", "--abbrev-ref", "HEAD")).toBe("main");
});
