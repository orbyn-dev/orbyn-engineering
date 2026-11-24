import { afterEach, expect, it, vi } from "vitest";
import { sendsPersonKey } from "../../api/personKey";
import { PINNED_LIMITS } from "../../../mock/library";
import { libraryApi, setLibraryApi } from "./api";

afterEach(() => { setLibraryApi(null); });
const answer = (status: number, body: unknown) => vi.fn(async (_url: string, _init?: RequestInit) => new Response(status === 204 ? null : JSON.stringify(body), { status, headers: { "content-type": "application/json" } }));
const sent = (fetching: ReturnType<typeof answer>) => { const [url, init] = fetching.mock.calls[0]!; return { url: String(url), method: init?.method, body: init?.body ? JSON.parse(String(init.body)) : undefined, raw: String(init?.body ?? "") }; };

it("answers a 404 or a foreign body from the summary as no feature, with no error", async () => {
  vi.stubGlobal("fetch", answer(404, { code: "not_found", message: "No such route." }));
  await expect(libraryApi().summary()).resolves.toBeNull();
  vi.stubGlobal("fetch", answer(200, { hello: "world" }));
  await expect(libraryApi().summary()).resolves.toBeNull();
});

it("lets any other failure through, so it is not mistaken for a missing feature", async () => {
  vi.stubGlobal("fetch", answer(500, { code: "tool_failed", message: "x" }));
  await expect(libraryApi().summary()).rejects.toMatchObject({ code: "tool_failed" });
});

it("reads each list as an array of objects and drops anything else", async () => {
  vi.stubGlobal("fetch", answer(200, [{ id: "a", name: "A" }, "junk", null, 4]));
  await expect(libraryApi().workflows()).resolves.toHaveLength(1);
  vi.stubGlobal("fetch", answer(200, { not: "an array" }));
  await expect(libraryApi().prompts()).resolves.toEqual([]);
});

it("sends an environment value only in the body of the key route, with the ids and names encoded", async () => {
  const fetching = answer(200, { name: "API_TOKEN", present: true });
  vi.stubGlobal("fetch", fetching);
  await libraryApi().setKey("01j/e 1", "API_TOKEN", "s3cr3t value");
  const call = sent(fetching);
  expect(call.method).toBe("PUT");
  expect(call.url).toContain("/api/library/env-sets/01j%2Fe%201/keys/API_TOKEN");
  expect(call.url).not.toContain("s3cr3t");
  expect(call.body).toEqual({ value: "s3cr3t value" });
});

it("renders with only the values in the body, and runs with a thread id", async () => {
  let fetching = answer(200, { command: "x" });
  vi.stubGlobal("fetch", fetching);
  await libraryApi().renderWorkflow("w1", { package: "auth" });
  expect(sent(fetching)).toMatchObject({ method: "POST", body: { values: { package: "auth" } } });
  expect(sent(fetching).url).toContain("/api/library/workflows/w1/render");
  fetching = answer(200, { terminal_id: "t" });
  vi.stubGlobal("fetch", fetching);
  await libraryApi().runWorkflow("w1", { package: "auth" }, { thread_id: "th 1" });
  expect(sent(fetching)).toMatchObject({ method: "POST", body: { values: { package: "auth" }, thread_id: "th 1" } });
});

it("reads the suggestions defensively and copies one by file and index", async () => {
  let fetching = answer(200, { workflows: [{ file: "workflows.json", index: 0, name: "W" }], prompts: "no", problems: [{ file: "x", index: 1, code: "c" }] });
  vi.stubGlobal("fetch", fetching);
  const found = await libraryApi().suggestions("p 1");
  expect(sent(fetching).url).toContain("/api/projects/p%201/library/suggestions");
  expect(found.workflows).toHaveLength(1);
  expect(found.prompts).toEqual([]);
  expect(found.env_sets).toEqual([]);
  expect(found.workflows[0]).toMatchObject({ file: "workflows.json", index: 0 });
  expect(found.problems).toEqual([{ file: "x", index: 1, code: "c" }]);
  const entry = { id: "01je", name: "staging", description: "", keys: [{ name: "API_URL", present: false }], keychain: "ok", status: "stored", revision: 1, created_at: "t", updated_at: "t" };
  fetching = answer(201, { kind: "env_set", entry });
  vi.stubGlobal("fetch", fetching);
  const copied = await libraryApi().copySuggestion("p1", "env-sets.json", 2);
  expect(sent(fetching)).toMatchObject({ method: "POST", body: { file: "env-sets.json", index: 2 } });
  expect(copied).toEqual({ kind: "env_set", entry });
});

it("reads the flat limits of the summary under the doc's keys", async () => {
  vi.stubGlobal("fetch", answer(200, { workflows: 1, prompts: 0, env_sets: 0, keychain: "ok", limits: PINNED_LIMITS }));
  const summary = await libraryApi().summary();
  expect(Object.keys(summary!.limits).sort()).toEqual(["env_keys_per_set", "env_sets", "env_sets_per_workflow", "env_value_bytes", "import_bytes", "params_per_item", "prompt_body_bytes", "prompts", "suggestions_file_bytes", "workflow_command_bytes", "workflows"]);
  expect(summary!.limits).toMatchObject({ workflows: 200, prompts: 200, env_sets: 50, env_keys_per_set: 64, env_sets_per_workflow: 4, params_per_item: 16, workflow_command_bytes: 4000, prompt_body_bytes: 32768, env_value_bytes: 8192, suggestions_file_bytes: 65536, import_bytes: 2097152 });
  expect(summary!.limits).not.toHaveProperty("env_keys");
  expect(summary!.limits).not.toHaveProperty("entry_bytes");
});

it("reads the run answer as a terminal id and a 409 confirm_required with its grants", async () => {
  vi.stubGlobal("fetch", answer(409, { code: "confirm_required", message: "Confirm.", detail: { grants: ["full_access"] } }));
  await expect(libraryApi().runWorkflow("w1", {}, { project_id: "p" })).rejects.toMatchObject({ code: "confirm_required", status: 409, detail: { grants: ["full_access"] } });
});

it("treats every library write as Person-class and the reads and renders as not", () => {
  for (const [method, path] of [["POST", "/api/library/workflows"], ["PATCH", "/api/library/workflows/w"], ["DELETE", "/api/library/prompts/p"], ["POST", "/api/library/workflows/w/run"], ["PUT", "/api/library/env-sets/e/keys/K"], ["DELETE", "/api/library/env-sets/e/keys/K"], ["POST", "/api/library/import"], ["POST", "/api/projects/p/library/suggestions/copy"]] as const) expect(sendsPersonKey(method, path), `${method} ${path}`).toBe(true);
  for (const [method, path] of [["GET", "/api/library"], ["GET", "/api/library/workflows"], ["POST", "/api/library/workflows/w/render"], ["POST", "/api/library/prompts/p/render"], ["GET", "/api/library/env-sets/e"], ["GET", "/api/projects/p/library/suggestions"], ["GET", "/api/library/export"]] as const) expect(sendsPersonKey(method, path), `${method} ${path}`).toBe(false);
});

it("sends ack_grants with every run: none for a first try, and exactly the list given for a resend", async () => {
  let fetching = answer(200, { terminal_id: "t1" });
  vi.stubGlobal("fetch", fetching);
  await expect(libraryApi().runWorkflow("w1", { a: "b" }, { project_id: "p 1" })).resolves.toEqual({ terminal_id: "t1" });
  expect(sent(fetching)).toMatchObject({ method: "POST", body: { values: { a: "b" }, project_id: "p 1", ack_grants: [] } });
  expect(sent(fetching).body).not.toHaveProperty("thread_id");
  fetching = answer(200, { terminal_id: "t2" });
  vi.stubGlobal("fetch", fetching);
  await libraryApi().runWorkflow("w1", {}, { thread_id: "th" }, ["env_values", "untrusted_source"]);
  expect(sent(fetching).body.ack_grants).toEqual(["env_values", "untrusted_source"]);
  expect(sent(fetching).url).toContain("/api/library/workflows/w1/run");
});

it("refuses a run answer that is not a terminal id, and keeps the 409 confirm_required detail as the service sent it", async () => {
  vi.stubGlobal("fetch", answer(200, { ok: true }));
  await expect(libraryApi().runWorkflow("w1", {}, { thread_id: "t" })).rejects.toMatchObject({ code: "bad_answer" });
  const detail = { grants: ["env_values"], env_sets: [{ name: "Staging", keys: ["API_TOKEN"] }] };
  vi.stubGlobal("fetch", answer(409, { code: "confirm_required", message: "Confirm.", detail }));
  await expect(libraryApi().runWorkflow("w1", {}, { thread_id: "t" })).rejects.toMatchObject({ code: "confirm_required", status: 409, detail });
});

it("imports a document with dry_run beside it, and reads the report", async () => {
  const report = { dry_run: true, problems: [], created: 1, renamed: 0, refused: 0, lines: [{ kind: "workflow", index: 0, outcome: "created", name: "W" }] };
  const fetching = answer(200, report);
  vi.stubGlobal("fetch", fetching);
  await expect(libraryApi().importLibrary({ format: "orbyn-library", version: 1, workflows: [{ name: "W" }] }, true)).resolves.toEqual(report);
  expect(sent(fetching)).toMatchObject({ method: "POST", body: { format: "orbyn-library", dry_run: true } });
  expect(sent(fetching).url).toContain("/api/library/import");
});

it("answers the 503 keychain_unavailable of a key write with its code and the service's message", async () => {
  vi.stubGlobal("fetch", answer(503, { code: "keychain_unavailable", message: "There is no usable keychain on this computer yet, so values cannot be saved." }));
  await expect(libraryApi().setKey("e1", "K", "v")).rejects.toMatchObject({ code: "keychain_unavailable", status: 503, message: expect.stringContaining("no usable keychain") });
});

it("reads an env set status of stored or planned as the service says it, with key names only", async () => {
  const planned = { id: "e", name: "S", description: "", keys: [{ name: "K", present: false }], keychain: "unavailable", status: "planned", revision: 1, created_at: "t", updated_at: "t" };
  vi.stubGlobal("fetch", answer(200, [planned]));
  const sets = await libraryApi().envSets();
  expect(sets[0]).toMatchObject({ status: "planned", keychain: "unavailable", keys: [{ name: "K", present: false }] });
});

it("reads suggestions whose workflows name their sets", async () => {
  vi.stubGlobal("fetch", answer(200, { workflows: [{ file: "workflows.json", index: 2, name: "W", description: "", command: "x", params: [], env_set_names: ["Staging"] }], prompts: [], env_sets: [], problems: [] }));
  const found = await libraryApi().suggestions("p1");
  expect(found.workflows[0]!.env_set_names).toEqual(["Staging"]);
});
