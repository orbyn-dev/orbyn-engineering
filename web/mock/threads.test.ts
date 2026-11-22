import { expect, it } from "vitest";
import { createMockApi } from "./api";

it("lists a checkout tree and reads files for the Files surface", () => {
  const api = createMockApi();
  const root = api.handle("GET", "/api/threads/t-approval/files?path=");
  expect(root.status).toBe(200);
  expect(Array.isArray(root.body)).toBe(true);
  expect((root.body as { path: string }[]).map((entry) => entry.path)).toContain("src");
  const nested = api.handle("GET", "/api/threads/t-approval/files?path=src");
  expect((nested.body as { path: string }[]).map((entry) => entry.path)).toEqual(["src/main.rs", "src/lib.rs"]);
  const file = api.handle("GET", "/api/threads/t-approval/files/content?path=src/main.rs");
  expect(file.body).toMatchObject({ path: "src/main.rs", binary: false, truncated: false });
  // A subpath must not fall back to the thread object, which crashed the Files tree.
  expect(api.handle("GET", "/api/threads/t-approval/previews").body).toEqual([]);
});

it("starts a side chat from a thread", () => {
  const api = createMockApi();
  const side = api.handle("POST", "/api/threads/t-approval/fork", { side: true });
  expect(side.status).toBe(201);
  expect(side.body).toMatchObject({ lifecycle: { parent_thread_id: "t-approval", relation: "side" } });
  expect(api.handle("POST", "/api/threads/missing/fork", { side: true }).status).toBe(404);
});

it("serves a thread's linked pull requests, and their refresh, as arrays", () => {
  const api = createMockApi();
  const linked = api.handle("GET", "/api/threads/t-approval/pulls");
  expect(linked.status).toBe(200);
  expect(linked.body).toEqual([expect.objectContaining({ number: 42, title: "Add approval timeouts" })]);
  const refreshed = api.handle("POST", "/api/threads/t-approval/pulls/refresh");
  expect(refreshed.status).toBe(200);
  expect(Array.isArray(refreshed.body)).toBe(true);
});

it("exports a thread as a transcript document", () => {
  const api = createMockApi();
  const answer = api.handle("GET", "/api/threads/t-done/export");
  expect(answer.status).toBe(200);
  expect(answer.body).toMatchObject({
    format: "orbyn-transcript",
    version: 1,
    thread: { title: "Explain how the event log resumes", engine: "claude" },
  });
  expect(Array.isArray((answer.body as { messages: unknown[] }).messages)).toBe(true);
  expect(api.handle("GET", "/api/threads/missing/export").status).toBe(404);
});

it("dry-runs and then imports a transcript into a project", () => {
  const api = createMockApi();
  const file = {
    format: "orbyn-transcript",
    version: 1,
    exported_at: "2026-11-22T10:00:00Z",
    thread: { title: "Redirect loop on logout", engine: "codex", created_at: "2026-11-20T09:00:00Z" },
    messages: [
      { role: "user", ts: "2026-11-20T09:00:05Z", text: "The logout page redirects back to itself." },
      { role: "assistant", ts: "2026-11-20T09:00:10Z", text: "The cookie is cleared after the redirect." },
    ],
    omitted_earlier: 0,
  };
  const dry = api.handle("POST", "/api/threads/import?project_id=p-app&engine=claude&dry_run=true&name=Redirect%20loop.orbyn.json", file);
  expect(dry.status).toBe(200);
  expect(dry.body).toMatchObject({ dry_run: true, thread: null, title: "Redirect loop on logout", message_count: 2, dropped: 0 });
  expect(api.threads.some((thread) => thread.title === "Redirect loop on logout")).toBe(false);

  const imported = api.handle("POST", "/api/threads/import?project_id=p-app&engine=codex&name=Redirect%20loop.orbyn.json", file);
  expect(imported.status).toBe(201);
  expect(imported.body).toMatchObject({
    dry_run: false,
    thread: { title: "Redirect loop on logout", engine: "codex", project_id: "p-app", source: "orbyn-transcript" },
  });
  expect((imported.body as { thread: { imported_at?: string } }).thread.imported_at).toBeTruthy();
  expect(api.threads.some((thread) => thread.title === "Redirect loop on logout" && thread.source === "orbyn-transcript")).toBe(true);
});

it("reports dropped messages for a file name that asks for the sample", () => {
  const api = createMockApi();
  const answer = api.handle("POST", "/api/threads/import?project_id=p-app&engine=claude&dry_run=true&name=dropped.orbyn.json", {});
  expect(answer.status).toBe(200);
  expect(answer.body).toMatchObject({
    dry_run: true,
    message_count: 211,
    dropped: 3,
  });
  expect((answer.body as { problems: unknown[] }).problems).toHaveLength(3);
});

it("refuses the import states the sheet draws", () => {
  const api = createMockApi();
  const call = (name: string) => api.handle("POST", `/api/threads/import?project_id=p-app&engine=claude&dry_run=true&name=${name}`, {});
  expect(call("huge.orbyn.json")).toMatchObject({ status: 413, body: { code: "file_too_large" } });
  expect(call("newer.orbyn.json")).toMatchObject({ status: 422, body: { code: "transcript_too_new" } });
  expect(call("notes.orbyn.json")).toMatchObject({ status: 422, body: { code: "transcript_unreadable" } });
  expect(call("nokey.orbyn.json")).toMatchObject({ status: 403, body: { code: "person_required" } });
  expect(api.handle("POST", "/api/threads/import?project_id=p-app&engine=missing&dry_run=true&name=x.orbyn.json", {})).toMatchObject({
    status: 404,
  });
});

it("serves the linked pull request's forge comments with one unresolved and one resolved thread", () => {
  const api = createMockApi();
  const response = api.handle("GET", "/api/projects/orbyn-engineering-app/pulls/42/comments");
  expect(response.status).toBe(200);
  const threads = (response.body as { comments: unknown[]; threads: { resolved: boolean; comments: unknown[] }[] }).threads;
  expect(threads.map((thread) => thread.resolved)).toEqual([false, true]);
  expect(threads.every((thread) => thread.comments.length > 0)).toBe(true);
});
