import { expect, it } from "vitest";
import type { ReposView } from "../src/generated/ReposView";
import type { TaskFolder } from "../src/generated/TaskFolder";
import type { ThreadRepo } from "../src/generated/ThreadRepo";
import { createMockApi } from "./api";
import * as fixtures from "./fixtures";
import { multiRepoThreadId } from "./multiRepo";
import { createReposMock } from "./repos";

it("answers a project's repositories as a ReposView, not the plain project list", () => {
  const api = createMockApi();
  const answer = api.handle("GET", "/api/projects/p-app/repos");
  expect(answer.status).toBe(200);
  const view = answer.body as ReposView;
  expect(view.repositories.map((row) => [row.id, row.primary, row.trusted])).toEqual([
    ["p-app", true, true],
    ["orbyn-planner", false, true],
    ["orbyn-engine", false, true],
  ]);
  expect(view.max_per_thread).toBe(8);
  // Another project answers its own folder, and a missing project is 404 rather than a project list.
  expect(((api.handle("GET", "/api/projects/p-site/repos").body) as ReposView).repositories.map((row) => [row.id, row.primary])).toEqual([["p-site", true]]);
  expect(api.handle("GET", "/api/projects/missing/repos").status).toBe(404);
});

it("answers the multi-repository thread's repositories and task folder, and 404 for an ordinary thread", () => {
  const api = createMockApi();
  const answer = api.handle("GET", `/api/threads/${multiRepoThreadId}/repos`);
  expect(answer.status).toBe(200);
  expect(Array.isArray(answer.body)).toBe(true);
  const repos = answer.body as ThreadRepo[];
  expect(repos.map((repo) => [repo.repo_id, repo.name, repo.branch, repo.trusted])).toEqual([
    ["p-app", "orbyn-engineering-app", "orbyn/1006-repos", true],
    ["orbyn-planner", "orbyn-planner", "orbyn/1006-repos", true],
  ]);
  expect(repos[0]).toMatchObject({ added: 4, deleted: 1, files: ["auth/redirect.ts", "auth/redirect.test.ts"] });

  const folder = api.handle("GET", `/api/threads/${multiRepoThreadId}/task-folder`).body as TaskFolder;
  expect(folder).toMatchObject({ branch: "orbyn/1006-repos", max_per_thread: 8 });
  expect(folder.repos.map((repo) => repo.repo_id)).toEqual(["p-app", "orbyn-planner"]);

  // An ordinary thread is 404: the read must never fall back to the thread object the Repos chip crashed on.
  const ordinary = api.handle("GET", "/api/threads/t-approval/repos");
  expect(ordinary).toMatchObject({ status: 404, body: { code: "not_found" } });
  expect((ordinary.body as { title?: string }).title).toBeUndefined();
  expect(api.handle("GET", "/api/threads/t-approval/task-folder").status).toBe(404);
  expect(api.handle("GET", "/api/threads/missing/repos").status).toBe(404);
});

it("draws the preview mock's Repos chip on the dev mock's multi-repository thread", async () => {
  expect(fixtures.threads().some((thread) => thread.id === multiRepoThreadId)).toBe(true);
  const preview = createReposMock();
  expect((await preview.threadRepos(multiRepoThreadId))?.map((repo) => repo.repo_id)).toEqual(["orbyn-engineering", "orbyn-planner"]);
  // The chip tests' own sample still answers under its id.
  expect(await preview.threadRepos("thread-1")).toHaveLength(2);
});
