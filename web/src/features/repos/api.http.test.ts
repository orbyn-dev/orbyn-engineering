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

it("sends nothing for the thread Git actions and removing a repository from a thread, which the document does not define", async () => {
  const fetching = stub();
  await expect(reposApi().threadGit("t1", "r1", { kind: "push" })).rejects.toMatchObject({ code: "not_offered" });
  await expect(reposApi().removeThreadRepo("t1", "r1")).rejects.toMatchObject({ code: "not_offered" });
  expect(fetching).not.toHaveBeenCalled();
});
