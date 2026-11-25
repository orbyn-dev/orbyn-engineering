import { expect, test } from "@playwright/test";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { startProject, type Service } from "./service";

/**
 * R1 Projects (10) and the file half of R2, through the real service routes: file listing, ignored
 * files, read/write with a revision, name and content search, mutate (create, move, delete), new
 * project creation, a scratch thread and its adopt, the folder browser, file metadata, the editor
 * list, open-in-editor refusals and the attachment upload/read/delete routes.
 */
test.describe.configure({ mode: "serial" });

let service: Service;
let projectId = "";
let threadId = "";
let engineId = "";

interface FileContent { path: string; content: string; revision: string; size: number; binary: boolean; truncated: boolean }
interface ExplorerEntry { name: string; path: string; is_dir: boolean }
interface Project { id: string; path: string; name: string }
interface Attachment { id: string; thread_id: string; name: string; mime: string; size: number }

const token = () => new URL(service.page).searchParams.get("token") ?? "";
async function raw(method: string, path: string, body?: BodyInit, headers: Record<string, string> = {}): Promise<Response> {
  return fetch(`${service.url}${path}`, { method, headers: { Authorization: `Bearer ${token()}`, ...headers }, body });
}
const api = <T,>(method: string, path: string, body?: unknown) => service.api<T>(method, path, body);
const createThread = (body: Record<string, unknown>) => api<{ id: string }>("POST", "/api/threads", { model: null, title: "RPC files", worktree: null, skill_profile: null, options: {}, ...body });

test.beforeAll(async () => {
  const started = await startProject({ fakeEngines: true });
  service = started.service;
  projectId = started.projectId;
  engineId = started.engines[0]!.id;
  threadId = (await createThread({ project_id: projectId, engine: engineId })).id;
});
test.afterAll(() => service.stop());

test("the files routes list, read, write, search and mutate the thread's checkout", async () => {
  const tree = await api<ExplorerEntry[]>("GET", `/api/threads/${threadId}/files`);
  expect(Array.isArray(tree)).toBe(true);
  expect(tree.some((entry) => entry.name === "README.md" && !entry.is_dir)).toBe(true);
  const ignored = await api<unknown[]>("GET", `/api/threads/${threadId}/files/ignored`);
  expect(Array.isArray(ignored)).toBe(true);

  const read = await api<FileContent>("GET", `/api/threads/${threadId}/files/content?path=README.md`);
  expect(read).toMatchObject({ path: "README.md", content: "# Repo\n", binary: false });
  expect(read.revision).not.toBe("");

  const written = await api<FileContent>("PUT", `/api/threads/${threadId}/files/content`, { path: "README.md", content: "# Repo\nEdited by rpc-files\n", base_revision: read.revision });
  expect(written.content).toContain("Edited by rpc-files");
  expect(readFileSync(join(service.repo, "README.md"), "utf8")).toContain("Edited by rpc-files");

  const contentHits = await api<{ path: string; line: number; text: string }[]>("GET", `/api/threads/${threadId}/files/search?q=Edited&content=true`);
  expect(contentHits.some((hit) => hit.path === "README.md" && hit.text.includes("Edited"))).toBe(true);
  const nameHits = await api<{ path: string }[]>("GET", `/api/threads/${threadId}/files/search?q=READ`);
  expect(nameHits.some((hit) => hit.path === "README.md")).toBe(true);

  expect(await api<void>("POST", `/api/threads/${threadId}/files/mutate`, { action: "create_file", path: "rpc-created.txt" })).toBeUndefined();
  expect(existsSync(join(service.repo, "rpc-created.txt"))).toBe(true);
  await api<void>("POST", `/api/threads/${threadId}/files/mutate`, { action: "move", path: "rpc-created.txt", destination: "rpc-moved.txt" });
  expect(existsSync(join(service.repo, "rpc-moved.txt"))).toBe(true);
  expect(existsSync(join(service.repo, "rpc-created.txt"))).toBe(false);
  await api<void>("POST", `/api/threads/${threadId}/files/mutate`, { action: "delete", path: "rpc-moved.txt" });
  expect(existsSync(join(service.repo, "rpc-moved.txt"))).toBe(false);
});

test("a new project is created and a scratch thread is adopted into a project", async () => {
  const created = await api<Project>("POST", "/api/projects/create", { name: "rpc-created-project", readme: true });
  expect(basename(created.path)).toBe("rpc-created-project");
  expect(existsSync(join(created.path, "README.md"))).toBe(true);
  expect(existsSync(join(created.path, ".git"))).toBe(true);

  const scratch = await createThread({ engine: engineId, title: "RPC scratch thread" });
  expect(scratch.id).not.toBe("");
  const adopted = await api<Project>("POST", `/api/threads/${scratch.id}/adopt`, { name: "RPC adopted project" });
  expect(adopted.name).toBe("RPC adopted project");
  const thread = await api<{ project_id?: string }>("GET", `/api/threads/${scratch.id}`);
  expect(thread.project_id).toBe(adopted.id);
});

test("the browser, metadata, editors and attachments routes answer", async () => {
  const browsing = await api<{ path: string; parent: string | null; entries: { name: string; is_git: boolean }[] }>("GET", `/api/fs/browse?path=${encodeURIComponent(dirname(service.repo))}`);
  expect(browsing.entries.some((entry) => entry.name === basename(service.repo) && entry.is_git)).toBe(true);
  expect(browsing.parent).not.toBeNull();
  expect(readdirSync(dirname(service.repo)).length).toBeGreaterThan(0);

  const metadata = await api<{ exists: boolean; kind: string | null; read_only: boolean }>("GET", `/api/threads/${threadId}/files/metadata?path=README.md`);
  expect(metadata.exists).toBe(true);
  expect(metadata.kind).not.toBeNull();
  expect(typeof metadata.read_only).toBe("boolean");
  const missing = await api<{ exists: boolean }>("GET", `/api/threads/${threadId}/files/metadata?path=rpc-missing.txt`);
  expect(missing.exists).toBe(false);
  const editors = await api<unknown[]>("GET", "/api/editors");
  expect(Array.isArray(editors)).toBe(true);
  const refused = await raw("POST", `/api/threads/${threadId}/files/open`, JSON.stringify({ path: "README.md", editor: "definitely-not-installed" }), { "Content-Type": "application/json" });
  // An unknown editor is an invalid request from Core, not a missing route.
  expect(refused.status).toBe(400);

  const uploaded = await raw("POST", `/api/threads/${threadId}/attachments?name=rpc.txt`, "hello rpc", { "Content-Type": "text/plain" });
  expect(uploaded.status).toBe(201);
  const attachment = (await uploaded.json()) as Attachment;
  expect(attachment.name).toBe("rpc.txt");
  expect(attachment.size).toBe(9);
  const bytes = await raw("GET", `/api/attachments/${attachment.id}/content`);
  expect(await bytes.text()).toBe("hello rpc");
  const listed = await api<Attachment[]>("GET", `/api/threads/${threadId}/attachments`);
  expect(listed.some((entry) => entry.id === attachment.id)).toBe(true);
  const removed = await raw("DELETE", `/api/attachments/${attachment.id}`);
  expect(removed.status).toBe(204);
  const gone = await raw("GET", `/api/attachments/${attachment.id}/content`);
  expect(gone.status).toBe(404);
});
