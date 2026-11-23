import { afterEach, expect, it, vi } from "vitest";
import { PERSON_HEADER } from "../../api/personKey";
import { reposApi, setReposApi } from "./api";

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
const row = { id: "r1", name: "r1", path: "/r1", is_git: true, trusted: false, branch: "main", branches: ["main"], has_remote: false, primary: false, clone: null };
afterEach(() => { setReposApi(null); vi.unstubAllGlobals(); });
const stub = (...answers: Response[]) => {
  const fetching = vi.fn(async () => answers.shift() ?? json(row));
  vi.stubGlobal("fetch", fetching);
  return fetching;
};
const call = (fetching: ReturnType<typeof stub>, index = 0) => {
  const [url, init] = fetching.mock.calls[index] as unknown as [string, RequestInit];
  return { method: init.method, path: url.replace(/^.*?(?=\/api)/, ""), body: init.body === undefined ? undefined : JSON.parse(String(init.body)), key: new Headers(init.headers).get(PERSON_HEADER) };
};

it("answers null, with no error, when the list is 404, and then asks for no thread's repositories", async () => {
  const fetching = stub(json({ code: "not_found", message: "x" }, 404));
  expect(await reposApi().list("p1")).toBeNull();
  expect(await reposApi().threadRepos("t1")).toBeNull();
  expect(fetching).toHaveBeenCalledTimes(1);
  expect(call(fetching)).toMatchObject({ method: "GET", path: "/api/projects/p1/repos", key: null });
});

it("reads the list and the thread's repositories without the person key, and a 404 thread is null", async () => {
  const view = { repositories: [row], clone_directory: null, max_per_thread: 8 };
  const fetching = stub(json(view), json([]), json({ code: "not_found", message: "x" }, 404));
  expect(await reposApi().list("p1")).toEqual(view);
  expect(await reposApi().threadRepos("t1")).toEqual([]);
  expect(await reposApi().threadRepos("t2")).toBeNull();
  expect(call(fetching, 1)).toMatchObject({ method: "GET", path: "/api/threads/t1/repos", key: null });
});

it("treats an answer that is not a repository list as nothing", async () => {
  stub(json({ ok: true }));
  expect(await reposApi().list("p1")).toBeNull();
});

it("lets any other failure of the list through", async () => {
  stub(json({ code: "internal", message: "boom" }, 500));
  await expect(reposApi().list("p1")).rejects.toMatchObject({ code: "internal", status: 500 });
});

it("calls each write on its route of docs/multi-repo.md, with the person key and the documented body", async () => {
  const fetching = stub();
  const api = reposApi();
  await api.addFolder("p1", "/work/a");
  await api.create("p1", "docs");
  await api.clone("p1", "https://github.com/o/r.git");
  await api.cancelClone("p1", "r1");
  await api.publish("p1", "r1", { name: "o/r", visibility: "private" });
  await api.trust("p1", "r1");
  await api.setupGit("p1", "r1");
  await api.addThreadRepo("t1", "r1");
  fetching.mockImplementationOnce(async () => new Response(null, { status: 204 }));
  await api.remove("p1", "r1");
  fetching.mockImplementationOnce(async () => json({ id: "t9" }));
  await api.startThread("p1", { repos: [{ repo_id: "r1", base_branch: "main" }], engine: "claude", model: null, permission_mode: "supervised", message: "hi" });
  const seen = fetching.mock.calls.map((_, index) => call(fetching, index));
  expect(seen.map(({ method, path, body }) => [method, path, body])).toEqual([
    ["POST", "/api/projects/p1/repos/add-folder", { path: "/work/a" }],
    ["POST", "/api/projects/p1/repos/create", { name: "docs" }],
    ["POST", "/api/projects/p1/repos/clone", { source: "https://github.com/o/r.git" }],
    ["POST", "/api/projects/p1/repos/r1/clone/cancel", undefined],
    ["POST", "/api/projects/p1/repos/r1/publish", { name: "o/r", visibility: "private" }],
    ["POST", "/api/projects/p1/repos/r1/trust", undefined],
    ["POST", "/api/projects/p1/repos/r1/setup-git", undefined],
    ["POST", "/api/threads/t1/repos", { repo_id: "r1" }],
    ["POST", "/api/projects/p1/repos/r1/remove", undefined],
    ["POST", "/api/projects/p1/repo-threads", { repos: [{ repo_id: "r1", base_branch: "main" }], engine: "claude", model: null, permission_mode: "supervised", message: "hi" }],
  ]);
  expect(seen.every((one) => one.key === "mock-person-key")).toBe(true);
});

it("encodes ids into the path", async () => {
  const fetching = stub();
  await reposApi().trust("p/1", "../x");
  expect(call(fetching).path).toBe("/api/projects/p%2F1/repos/..%2Fx/trust");
});

it("sends each thread Git action on the existing route, with repo_id for a chosen repository and none for the primary", async () => {
  const phases = { phases: [{ name: "commit", ok: true, message: null, url: null }], notice: null };
  const fetching = stub(...Array.from({ length: 4 }, () => json(phases)));
  const api = reposApi();
  await api.threadGit("t1", "r2", { kind: "commit", message: "Fix" });
  await api.threadGit("t1", "r2", { kind: "push" });
  await api.threadGit("t1", "r2", { kind: "pull_request", title: "T" });
  await api.threadGit("t1", null, { kind: "push" });
  const seen = fetching.mock.calls.map((_, index) => call(fetching, index));
  expect(seen.map(({ method, path, body }) => [method, path, body])).toEqual([
    ["POST", "/api/threads/t1/git/action", { commit_message: "Fix", push: false, repo_id: "r2" }],
    ["POST", "/api/threads/t1/git/action", { push: true, repo_id: "r2" }],
    ["POST", "/api/threads/t1/git/action", { push: true, pull_request: { title: "T", body: "" }, repo_id: "r2" }],
    ["POST", "/api/threads/t1/git/action", { push: true }],
  ]);
  // The git action route keeps the class it has: no person key.
  expect(seen.map((one) => one.key !== null)).toEqual([false, false, false, false]);
});

it("reads a failed phase of an action as a failure, and passes 404 and 409 through as they are", async () => {
  stub(json({ phases: [{ name: "push", ok: false, message: "rejected", url: null }], notice: null }));
  await expect(reposApi().threadGit("t1", "r2", { kind: "push" })).rejects.toMatchObject({ message: expect.stringContaining("rejected") });
  stub(json({ code: "repo_untrusted", message: "no" }, 409));
  await expect(reposApi().threadGit("t1", "r2", { kind: "push" })).rejects.toMatchObject({ code: "repo_untrusted", status: 409 });
  stub(json({ code: "not_found", message: "no" }, 404));
  await expect(reposApi().threadGit("t1", "zz", { kind: "push" })).rejects.toMatchObject({ code: "not_found", status: 404 });
});

it("removes a repository from a thread with DELETE and the person key, and passes the service's refusal through", async () => {
  const fetching = stub(new Response(null, { status: 204 }), json({ code: "invalid", message: "Commit or discard its changes first" }, 400));
  await expect(reposApi().removeThreadRepo("t 1", "r/2")).resolves.toBeUndefined();
  expect(call(fetching)).toMatchObject({ method: "DELETE", path: "/api/threads/t%201/repos/r%2F2" });
  expect(call(fetching).key).not.toBeNull();
  await expect(reposApi().removeThreadRepo("t1", "r2")).rejects.toMatchObject({ status: 400, message: "Commit or discard its changes first" });
});

it("scans through the Person-class route with the path in the body, and shapes the answer", async () => {
  const fetching = stub(json({ path: "/w", is_git: false, code_workspace_file: null, truncated: false, folders: [{ name: "a", path: "/w/a", is_git: true, source: "child", in_project: false, refused: null }] }));
  const scan = await reposApi().scan("p/1", "/w");
  expect(scan?.folders).toHaveLength(1);
  expect(call(fetching)).toMatchObject({ method: "POST", path: "/api/projects/p%2F1/repos/scan", body: { path: "/w" }, key: "mock-person-key" });
});

it("answers null for a scan or a task folder the service does not have (404), with no error", async () => {
  stub(json({ code: "not_found", message: "x" }, 404), json({ code: "not_found", message: "x" }, 404));
  expect(await reposApi().scan("p1", "/w")).toBeNull();
  expect(await reposApi().taskFolder("t1")).toBeNull();
});

it("reads a thread's task folder without the person key", async () => {
  const fetching = stub(json({ root: "/t/t1", branch: "feat/x", repos: [], max_per_thread: 8 }));
  expect(await reposApi().taskFolder("t1")).toMatchObject({ root: "/t/t1", branch: "feat/x" });
  expect(call(fetching)).toMatchObject({ method: "GET", path: "/api/threads/t1/task-folder", key: null });
});

it("sends branch_name on a thread start only when it is given", async () => {
  const fetching = stub(json({ id: "t9" }), json({ id: "t9" }));
  const base = { repos: [{ repo_id: "a", base_branch: "main" }, { repo_id: "b", base_branch: "main" }], engine: "claude", model: null, permission_mode: "supervised" as const };
  await reposApi().startThread("p1", { ...base, branch_name: "feat/x" });
  await reposApi().startThread("p1", base);
  expect(call(fetching, 0).body.branch_name).toBe("feat/x");
  expect("branch_name" in call(fetching, 1).body).toBe(false);
});

// Real service shapes (crates/orbynd/src/routes/repos.rs, docs/multi-repo.md "As built: project repositories").
it("reads the service's own clone object: state cloning or failed, percent absent, a message only after a failure", async () => {
  const cloning = { ...row, id: "c1", clone: { state: "cloning" } };
  const failed = { ...row, id: "c2", clone: { state: "failed", message: "The clone failed: no access." } };
  stub(json({ repositories: [row, cloning, failed], clone_directory: "/data/repos", max_per_thread: 8 }));
  const view = await reposApi().list("p1");
  expect(view?.repositories.map((one) => one.clone)).toEqual([null, { state: "cloning" }, { state: "failed", message: "The clone failed: no access." }]);
});

it("reads cancel as the failed row the service answers, and remove as an empty 204", async () => {
  const cancelled = { ...row, id: "c1", is_git: false, trusted: false, branch: null, branches: [], clone: { state: "failed", percent: null, message: "Cancelled." } };
  const fetching = stub(json(cancelled), new Response(null, { status: 204 }));
  expect(await reposApi().cancelClone("p1", "c1")).toEqual(cancelled);
  await expect(reposApi().remove("p1", "c1")).resolves.toBeUndefined();
  expect(call(fetching, 0)).toMatchObject({ method: "POST", path: "/api/projects/p1/repos/c1/clone/cancel", body: undefined });
  expect(call(fetching, 1)).toMatchObject({ method: "POST", path: "/api/projects/p1/repos/c1/remove", body: undefined });
});

it("passes the service's repository error codes through with their status", async () => {
  const cases: [number, string][] = [[409, "repo_exists"], [409, "repo_limit"], [400, "clone_refused"], [409, "repo_untrusted"], [400, "invalid"], [404, "not_found"]];
  for (const [status, code] of cases) {
    stub(json({ code, message: "no" }, status));
    await expect(reposApi().addFolder("p1", "/x")).rejects.toMatchObject({ code, status });
  }
});

it("reads publish's 422 unsupported as that failure, not as an answer", async () => {
  stub(json({ code: "unsupported", message: "Publishing a repository arrives in a later update." }, 422));
  await expect(reposApi().publish("p1", "r1", { name: "o/r", visibility: "private" })).rejects.toMatchObject({ code: "unsupported", status: 422, message: "Publishing a repository arrives in a later update." });
});

it("reads the scan's code_workspace source, and drops an entry whose source this page does not know (the old guess, code-workspace)", async () => {
  const entry = (name: string, source: string) => ({ name, path: `/w/${name}`, is_git: true, source, in_project: false, refused: null });
  stub(json({ path: "/w/x.code-workspace", is_git: false, code_workspace_file: "x.code-workspace", truncated: false, folders: [entry("a", "code_workspace"), entry("b", "child"), entry("c", "code-workspace")] }));
  const scan = await reposApi().scan("p1", "/w/x.code-workspace");
  expect(scan?.folders.map((folder) => [folder.name, folder.source])).toEqual([["a", "code_workspace"], ["b", "child"]]);
  expect(scan?.code_workspace_file).toBe("x.code-workspace");
});

// Real service shape of GET /api/threads/{id}/repos (crates/orbyn-core/src/repos/scope.rs): `files` is a list, or null for an untrusted repository.
it("reads the real thread repositories answer as it is: untrusted entries have no files, and no pull request is invented", async () => {
  const answer = [
    { repo_id: "p1", name: "app", branch: "feat/x", added: 3, deleted: 1, trusted: true, files: ["a.ts"] },
    { repo_id: "r2", name: "lib", branch: "feat/x", added: 0, deleted: 0, trusted: false, files: null },
  ];
  stub(json(answer), json({ not: "a list" }));
  expect(await reposApi().threadRepos("t1")).toEqual(answer);
  expect(await reposApi().threadRepos("t1")).toBeNull();
});

it("lets a refused thread repositories read through with the service's code", async () => {
  stub(json({ code: "repo_untrusted", message: "Trust this repository before using it." }, 409));
  await expect(reposApi().threadRepos("t1")).rejects.toMatchObject({ code: "repo_untrusted", status: 409 });
});

it("sends the generated StartThreadInput and AddThreadRepo bodies, and reads the generated TaskFolder", async () => {
  const folder = { root: "/data/threads/t1", branch: "orbyn/t1", repos: [{ repo_id: "a", name: "a", branch: "orbyn/t1", added: 0, deleted: 0, trusted: true, files: [] }], max_per_thread: 8 };
  const fetching = stub(json({ id: "t1" }), json({ repo_id: "b", name: "b", branch: "orbyn/t1", added: 0, deleted: 0, trusted: true }), json(folder));
  const api = reposApi();
  await api.startThread("p1", { repos: [{ repo_id: "a", base_branch: "main" }, { repo_id: "b", base_branch: "dev" }], engine: "claude", model: null, permission_mode: "supervised", message: "go", branch_name: "feat/x" });
  await api.addThreadRepo("t1", "b");
  expect((await api.taskFolder("t1"))?.max_per_thread).toBe(8);
  expect(call(fetching, 0)).toMatchObject({ method: "POST", path: "/api/projects/p1/repo-threads", key: expect.any(String), body: { repos: [{ repo_id: "a", base_branch: "main" }, { repo_id: "b", base_branch: "dev" }], engine: "claude", model: null, permission_mode: "supervised", message: "go", branch_name: "feat/x" } });
  expect(call(fetching, 1)).toMatchObject({ method: "POST", path: "/api/threads/t1/repos", body: { repo_id: "b" }, key: expect.any(String) });
  expect(call(fetching, 2)).toMatchObject({ method: "GET", path: "/api/threads/t1/task-folder", key: null });
});

it("passes the thread start and add refusals through with the service's code, status and message", async () => {
  const refusals: [string, number, string][] = [
    ["repo_limit", 409, "A thread works in at most 8 repositories."],
    ["repo_untrusted", 409, "Trust b before starting a thread in it."],
    ["not_git", 409, "b is not a Git repository."],
    ["invalid", 400, "b already has a branch named feat/x."],
    ["permission_mode_unsupported", 400, "no"],
  ];
  stub(...refusals.map(([code, status, message]) => json({ code, message }, status)), ...refusals.map(([code, status, message]) => json({ code, message }, status)));
  for (const [code, status, message] of refusals) {
    await expect(reposApi().startThread("p1", { repos: [], engine: "claude", model: null, permission_mode: "supervised" })).rejects.toMatchObject({ code, status, message });
  }
  for (const [code, status] of refusals.slice(0, 4)) await expect(reposApi().addThreadRepo("t1", "b")).rejects.toMatchObject({ code, status });
});
