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
  const fetching = stub(...Array.from({ length: 7 }, () => json(phases)));
  const api = reposApi();
  await api.threadGit("t1", "r2", { kind: "commit", message: "Fix" });
  await api.threadGit("t1", "r2", { kind: "push" });
  await api.threadGit("t1", "r2", { kind: "pull_request", title: "T" });
  await api.threadGit("t1", null, { kind: "push" });
  await api.threadGit("t1", "r2", { kind: "merge", number: 7, method: "squash" });
  await api.threadGit("t1", "r2", { kind: "review", number: 7, event: "approve", body: "ok" });
  await api.threadGit("t/1", null, { kind: "merge", number: 7, method: "merge" });
  const seen = fetching.mock.calls.map((_, index) => call(fetching, index));
  expect(seen.map(({ method, path, body }) => [method, path, body])).toEqual([
    ["POST", "/api/threads/t1/git/action", { commit_message: "Fix", push: false, repo_id: "r2" }],
    ["POST", "/api/threads/t1/git/action", { push: true, repo_id: "r2" }],
    ["POST", "/api/threads/t1/git/action", { push: true, pull_request: { title: "T", body: "" }, repo_id: "r2" }],
    ["POST", "/api/threads/t1/git/action", { push: true }],
    ["POST", "/api/threads/t1/pulls/7/merge", { method: "squash", auto: false, delete_branch: false, strip_credits: false, repo_id: "r2" }],
    ["POST", "/api/threads/t1/pulls/7/review", { event: "approve", body: "ok", comments: [], repo_id: "r2" }],
    ["POST", "/api/threads/t%2F1/pulls/7/merge", { method: "merge", auto: false, delete_branch: false, strip_credits: false }],
  ]);
  // Merge and review are Person-class routes: the key goes with them. The git action route keeps the class it has.
  expect(seen.map((one) => one.key !== null)).toEqual([false, false, false, false, true, true, true]);
});

it("reads a failed phase of an action as a failure, and passes 404 and 409 through as they are", async () => {
  stub(json({ phases: [{ name: "push", ok: false, message: "rejected", url: null }], notice: null }));
  await expect(reposApi().threadGit("t1", "r2", { kind: "push" })).rejects.toMatchObject({ message: expect.stringContaining("rejected") });
  stub(json({ code: "repo_untrusted", message: "no" }, 409));
  await expect(reposApi().threadGit("t1", "r2", { kind: "push" })).rejects.toMatchObject({ code: "repo_untrusted", status: 409 });
  stub(json({ code: "not_found", message: "no" }, 404));
  await expect(reposApi().threadGit("t1", "zz", { kind: "push" })).rejects.toMatchObject({ code: "not_found", status: 404 });
});

it("has no call that removes a repository from a thread", () => {
  expect("removeThreadRepo" in reposApi()).toBe(false);
});
