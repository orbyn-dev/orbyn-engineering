/**
 * MOCK of the library routes of docs/execution-profiles-and-library.md, Part 2, for tests and the dev-only preview (`#/preview/library`),
 * which install it through `setLibraryApi`. The app's own client is the real HTTP one in src/features/library/api.ts. Fixtures follow
 * the Engineer frames `workflow-*`. It refuses what the doc says the service refuses: `name_taken`, `revision_conflict`,
 * `library_limit`, `key_refused`, `invalid_input`, `keychain_unavailable`, `keychain_blocked`, `folder_untrusted`. Environment values
 * are held in a private map that no read returns, as the real keychain does: `present` is the only thing a read says.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { EnvSet, EnvSetDraft, LibraryApi, LibrarySummary, Param, Prompt, PromptDraft, Suggestions, Workflow, WorkflowDraft } from "../src/features/library/contract.ts";

const NOW = "2026-11-22T00:00:00Z";
const stamp = { revision: 1, created_at: NOW, updated_at: NOW };

export function libraryFixtures(): { workflows: Workflow[]; prompts: Prompt[]; envSets: EnvSet[] } {
  const pkg: Param = { name: "package", label: "Package", kind: "choice", default: "auth", required: true, choices: ["auth", "web", "docs"] };
  return {
    workflows: [
      { id: "01jw0000000000000000000001", name: "Test one package", description: "Runs the tests for one package", command: "npm test -- {{package}}", params: [pkg], env_set_ids: [], ...stamp },
      { id: "01jw0000000000000000000002", name: "Prune merged branches", description: "Deletes local branches that are already merged", command: "git branch --merged | grep -v main | xargs git branch -d", params: [], env_set_ids: [], ...stamp },
    ],
    prompts: [
      { id: "01jp0000000000000000000001", name: "Standup summary", description: "Summarises yesterday’s commits", body: "Summarise the commits since {{since}} in three lines.", params: [{ name: "since", label: "Since", kind: "text", default: "yesterday", required: true }], ...stamp },
    ],
    envSets: [
      { id: "01je0000000000000000000001", name: "work-laptop", description: "", keys: [{ name: "API_BASE", present: true }, { name: "API_TOKEN", present: false }], keychain: "ok", status: "stored", ...stamp },
    ],
  };
}

export function suggestionFixtures(): Suggestions {
  return {
    workflows: [{ index: 0, name: "Start dev server", description: "Starts the app on a port", command: "PORT={{port}} npm run dev", params: [{ name: "port", label: "Port", kind: "text", default: "5173", required: true }], env_set_ids: [] }],
    prompts: [{ index: 0, name: "Review a diff for open redirects", description: "Asks the engine to check a change", body: "Review {{diff}} for open redirects.", params: [{ name: "diff", label: "Diff", kind: "choice", default: "this branch", required: true, choices: ["this branch", "working tree"] }] }],
    env_sets: [{ index: 0, name: "staging", description: "", keys: ["API_URL", "DEPLOY_TOKEN"] }],
    problems: [{ file: "workflows.json", index: 3, code: "invalid_entry" }],
  };
}

export interface LibraryMock extends LibraryApi {
  /** Every write the screens made, in order. Values are never recorded: a key write is `key:<set>:<KEY>`. */
  writes: string[];
  /** The value the keychain holds for a key, for a test to check what was stored. The page can never call this. */
  vault: Map<string, string>;
  /** The last body a `setKey` call carried, for a test to check the value went only there. */
  lastKeyBody(): string | null;
  failNext(code: string | null, message?: string): void;
  /** Change a stored workflow as if another window had saved it. */
  touch(id: string): void;
}

const STATUS: Record<string, [number, string]> = {
  name_taken: [409, "That name is already used."],
  revision_conflict: [409, "This item changed since you opened it."],
  library_limit: [409, "The library is full for this kind."],
  key_refused: [409, "That variable name is refused."],
  invalid_input: [400, "Check the highlighted field."],
  keychain_unavailable: [503, "The keychain is not available."],
  keychain_blocked: [503, "The keychain is locked or the prompt was cancelled."],
  keychain_failed: [502, "The keychain could not finish."],
  folder_untrusted: [403, "This folder is not trusted yet."],
  not_found: [404, "That item is not in the library any more."],
};

export function createLibraryMock(init: { fixtures?: ReturnType<typeof libraryFixtures> | null; suggestions?: Suggestions | "untrusted"; platform?: "stored" | "planned"; delayMs?: number; limits?: { workflows?: number; prompts?: number; env_sets?: number } } = {}): LibraryMock {
  const missing = init.fixtures === null;
  const seed = init.fixtures ?? libraryFixtures();
  const workflows = new Map(seed.workflows.map((one) => [one.id, one] as const));
  const prompts = new Map(seed.prompts.map((one) => [one.id, one] as const));
  const sets = new Map(seed.envSets.map((one) => [one.id, one] as const));
  const vault = new Map<string, string>();
  const writes: string[] = [];
  let fail: [string, string | undefined] | null = null;
  let lastBody: string | null = null;
  let seq = 0;
  const wait = () => new Promise<void>((resolve) => setTimeout(resolve, init.delayMs ?? 0));
  const refuse = (code: string, message?: string): never => {
    const [status, text] = STATUS[code] ?? [502, "The service could not do that."];
    throw new ApiFailure(code, message ?? text, status);
  };
  const preflight = () => { if (fail) { const [code, message] = fail; fail = null; refuse(code, message); } };
  const nextId = (prefix: string) => `${prefix}${String((seq += 1)).padStart(25, "0")}`;
  const taken = (all: Map<string, { id: string; name: string }>, name: string, except: string | null) => [...all.values()].some((one) => one.id !== except && one.name.trim().toLowerCase() === name.trim().toLowerCase());
  const limit = (all: Map<string, unknown>, cap: number) => { if (all.size >= cap) refuse("library_limit"); };
  const fill = (text: string, params: Param[], values: Record<string, string>): string => {
    for (const param of params) if (param.required && !(Object.hasOwn(values, param.name) ? values[param.name] : param.default)) refuse("invalid_input");
    return text.replace(/\{\{\s*([a-z][a-z0-9_]*)\s*\}\}/g, (_all, name: string) => {
      const param = params.find((one) => one.name === name);
      if (!param) return refuse("invalid_input");
      const value = (Object.hasOwn(values, name) ? values[name] : undefined) ?? param.default ?? "";
      if (param.kind === "choice" && value !== "" && !(param.choices ?? []).includes(value)) refuse("invalid_input");
      return `'${value.replaceAll("'", "'\\''")}'`;
    });
  };
  const presentOf = (set: EnvSet): EnvSet => ({ ...set, status: init.platform ?? set.status, keychain: init.platform === "planned" ? "unavailable" : set.keychain, keys: set.keys.map((key) => ({ name: key.name, present: init.platform === "planned" ? false : vault.has(`${set.id}/${key.name}`) || (set.keys.find((k) => k.name === key.name)?.present ?? false) })) });
  const bump = <T extends { revision: number }>(one: T): T => ({ ...one, revision: one.revision + 1, updated_at: NOW });
  const summary = (): LibrarySummary => ({ workflows: workflows.size, prompts: prompts.size, env_sets: sets.size, keychain: init.platform === "planned" ? "unavailable" : "ok", limits: { workflows: init.limits?.workflows ?? 200, prompts: init.limits?.prompts ?? 200, env_sets: init.limits?.env_sets ?? 50, env_keys: 64 } });
  return {
    writes, vault,
    lastKeyBody: () => lastBody,
    failNext(code, message) { fail = code === null ? null : [code, message]; },
    touch(id) { const one = workflows.get(id) ?? refuse("not_found"); workflows.set(id, bump(one)); },
    async summary() { await wait(); return missing ? null : summary(); },
    async workflows() { await wait(); return [...workflows.values()]; },
    async prompts() { await wait(); return [...prompts.values()]; },
    async envSets() { await wait(); return [...sets.values()].map(presentOf); },
    async renderWorkflow(id, values) { await wait(); const one = workflows.get(id) ?? refuse("not_found"); return { command: fill(one.command, one.params, values) }; },
    async renderPrompt(id, values) { await wait(); const one = prompts.get(id) ?? refuse("not_found"); return { text: fill(one.body, one.params, values) }; },
    async createWorkflow(draft: WorkflowDraft) {
      await wait(); preflight();
      if (taken(workflows, draft.name, null)) refuse("name_taken");
      limit(workflows, init.limits?.workflows ?? 200);
      const one: Workflow = { ...draft, name: draft.name.trim(), id: nextId("01jwn"), ...stamp };
      workflows.set(one.id, one); writes.push(`workflow.create:${one.name}`); return one;
    },
    async updateWorkflow(id, draft, seen) {
      await wait(); preflight();
      const one = workflows.get(id) ?? refuse("not_found");
      if (seen !== one.revision) refuse("revision_conflict");
      if (taken(workflows, draft.name, id)) refuse("name_taken");
      const next = bump({ ...one, ...draft, name: draft.name.trim() });
      workflows.set(id, next); writes.push(`workflow.update:${next.name}`); return next;
    },
    async deleteWorkflow(id) { await wait(); preflight(); workflows.delete(id); writes.push(`workflow.delete:${id}`); },
    async runWorkflow(id, values, where) {
      await wait(); preflight();
      const one = workflows.get(id) ?? refuse("not_found");
      fill(one.command, one.params, values);
      writes.push(`workflow.run:${id}:${"thread_id" in where ? `thread=${where.thread_id}` : `project=${where.project_id}`}`);
      return { terminal_id: "term-1" };
    },
    async createPrompt(draft: PromptDraft) {
      await wait(); preflight();
      if (taken(prompts, draft.name, null)) refuse("name_taken");
      limit(prompts, init.limits?.prompts ?? 200);
      const one: Prompt = { ...draft, name: draft.name.trim(), id: nextId("01jpn"), ...stamp };
      prompts.set(one.id, one); writes.push(`prompt.create:${one.name}`); return one;
    },
    async updatePrompt(id, draft, seen) {
      await wait(); preflight();
      const one = prompts.get(id) ?? refuse("not_found");
      if (seen !== one.revision) refuse("revision_conflict");
      if (taken(prompts, draft.name, id)) refuse("name_taken");
      const next = bump({ ...one, ...draft, name: draft.name.trim() });
      prompts.set(id, next); writes.push(`prompt.update:${next.name}`); return next;
    },
    async deletePrompt(id) { await wait(); preflight(); prompts.delete(id); writes.push(`prompt.delete:${id}`); },
    async createEnvSet(draft: EnvSetDraft) {
      await wait(); preflight();
      if (taken(sets, draft.name, null)) refuse("name_taken");
      limit(sets, init.limits?.env_sets ?? 50);
      const one: EnvSet = { id: nextId("01jen"), name: draft.name.trim(), description: draft.description, keys: draft.keys.map((name) => ({ name, present: false })), keychain: "ok", status: init.platform ?? "stored", ...stamp };
      sets.set(one.id, one); writes.push(`env_set.create:${one.name}`); return presentOf(one);
    },
    async updateEnvSet(id, draft, seen) {
      await wait(); preflight();
      const one = sets.get(id) ?? refuse("not_found");
      if (seen !== one.revision) refuse("revision_conflict");
      const keep = new Map(one.keys.map((key) => [key.name, key] as const));
      const next = bump({ ...one, name: draft.name.trim(), description: draft.description, keys: draft.keys.map((name) => keep.get(name) ?? { name, present: false }) });
      sets.set(id, next); writes.push(`env_set.update:${next.name}`); return presentOf(next);
    },
    async deleteEnvSet(id) { await wait(); preflight(); sets.delete(id); for (const key of [...vault.keys()]) if (key.startsWith(`${id}/`)) vault.delete(key); writes.push(`env_set.delete:${id}`); },
    async setKey(setId, key, value) {
      await wait(); preflight();
      lastBody = JSON.stringify({ value });
      const one = sets.get(setId) ?? refuse("not_found");
      if (init.platform === "planned") refuse("keychain_unavailable");
      if (!one.keys.some((entry) => entry.name === key)) refuse("key_refused");
      if (value.length < 1 || value.length > 8192) refuse("invalid_input");
      vault.set(`${setId}/${key}`, value);
      sets.set(setId, { ...one, keys: one.keys.map((entry) => (entry.name === key ? { ...entry, present: true } : entry)) });
      writes.push(`key:${setId}:${key}`);
      return { name: key, present: true };
    },
    async removeKey(setId, key) {
      await wait(); preflight();
      const one = sets.get(setId) ?? refuse("not_found");
      vault.delete(`${setId}/${key}`);
      sets.set(setId, { ...one, keys: one.keys.map((entry) => (entry.name === key ? { ...entry, present: false } : entry)) });
      writes.push(`key.remove:${setId}:${key}`);
      return { name: key, present: false };
    },
    async suggestions() {
      await wait();
      if (init.suggestions === "untrusted") return refuse("folder_untrusted");
      return init.suggestions ?? suggestionFixtures();
    },
    async copySuggestion(_projectId, file, index) {
      await wait(); preflight();
      const found = init.suggestions && init.suggestions !== "untrusted" ? init.suggestions : suggestionFixtures();
      if (file === "workflows.json") { const one = found.workflows.find((entry) => entry.index === index) ?? refuse("not_found"); await this.createWorkflow({ name: one.name, description: one.description, command: one.command, params: one.params, env_set_ids: [] }); }
      else if (file === "prompts.json") { const one = found.prompts.find((entry) => entry.index === index) ?? refuse("not_found"); await this.createPrompt({ name: one.name, description: one.description, body: one.body, params: one.params }); }
      else { const one = found.env_sets.find((entry) => entry.index === index) ?? refuse("not_found"); await this.createEnvSet({ name: one.name, description: one.description, keys: one.keys }); }
    },
  };
}
