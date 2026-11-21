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
