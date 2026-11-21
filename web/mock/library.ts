/**
 * MOCK of the library routes of docs/execution-profiles-and-library.md, Part 2, for tests and the dev-only preview (`#/preview/library`),
 * which install it through `setLibraryApi`. The app's own client is the real HTTP one in src/features/library/api.ts. Fixtures follow
 * the Engineer frames `workflow-*`. It refuses what the doc says the service refuses: `name_taken`, `revision_conflict`,
 * `library_limit`, `key_refused`, `invalid_input`, `keychain_unavailable`, `keychain_blocked`, `folder_untrusted`, and `409 confirm_required` on a run (grants `env_values` and `untrusted_source`, set names with key names; the ack is stored on the entry until its command or parameters change). Environment values
 * are held in a private map that no read returns, as the real keychain does: `present` is the only thing a read says.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { Copied, EnvSet, EnvSetDraft, ImportAnswer, LibraryApi, LibraryLimits, LibrarySummary, Param, ProfileGrant, Prompt, PromptDraft, Suggestions, Workflow, WorkflowDraft } from "../src/features/library/contract.ts";

const NOW = "2026-11-22T00:00:00Z";
const stamp = { revision: 1, created_at: NOW, updated_at: NOW };
const local = { source: "local", needs_ack: false } as const;

export function libraryFixtures(): { workflows: Workflow[]; prompts: Prompt[]; envSets: EnvSet[] } {
  const pkg: Param = { name: "package", label: "Package", kind: "choice", default: "auth", required: true, choices: ["auth", "web", "docs"] };
  return {
    workflows: [
      { id: "01jw0000000000000000000001", name: "Test one package", description: "Runs the tests for one package", command: "npm test -- {{package}}", params: [pkg], env_set_ids: [], ...local, ...stamp },
      { id: "01jw0000000000000000000002", name: "Prune merged branches", description: "Deletes local branches that are already merged", command: "git branch --merged | grep -v main | xargs git branch -d", params: [], env_set_ids: [], ...local, ...stamp },
    ],
    prompts: [
      { id: "01jp0000000000000000000001", name: "Standup summary", description: "Summarises yesterday’s commits", body: "Summarise the commits since {{since}} in three lines.", params: [{ name: "since", label: "Since", kind: "text", default: "yesterday", required: true }], source: "local", ...stamp },
    ],
    envSets: [
      { id: "01je0000000000000000000001", name: "work-laptop", description: "", keys: [{ name: "API_BASE", present: true }, { name: "API_TOKEN", present: false }], keychain: "ok", status: "stored", ...stamp },
    ],
  };
}

export function suggestionFixtures(): Suggestions {
  return {
    workflows: [{ file: "workflows.json", index: 0, name: "Start dev server", description: "Starts the app on a port", command: "PORT={{port}} npm run dev", params: [{ name: "port", label: "Port", kind: "text", default: "5173", required: true }], env_set_names: [] }],
    prompts: [{ file: "prompts.json", index: 0, name: "Review a diff for open redirects", description: "Asks the engine to check a change", body: "Review {{diff}} for open redirects.", params: [{ name: "diff", label: "Diff", kind: "choice", default: "this branch", required: true, choices: ["this branch", "working tree"] }] }],
    env_sets: [{ file: "env-sets.json", index: 0, name: "staging", description: "", keys: ["API_URL", "DEPLOY_TOKEN"] }],
    problems: [{ file: "workflows.json", index: 3, code: "invalid_input" }],
  };
}

/** The numbers of "Limits" in the doc, under the doc's keys. */
export const PINNED_LIMITS: LibraryLimits = { workflows: 200, prompts: 200, env_sets: 50, env_keys_per_set: 64, env_sets_per_workflow: 4, params_per_item: 16, workflow_command_bytes: 4000, prompt_body_bytes: 32768, env_value_bytes: 8192, suggestions_file_bytes: 65536, import_bytes: 2097152 };

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
  keychain_unavailable: [503, "There is no usable keychain on this computer yet, so values cannot be saved."],
  keychain_blocked: [503, "The keychain is locked or the prompt was cancelled."],
  keychain_failed: [502, "The keychain could not finish."],
  folder_untrusted: [403, "This folder is not trusted yet."],
  confirm_required: [409, "Confirm what this run grants before it starts."],
  not_found: [404, "That item is not in the library any more."],
};

export function createLibraryMock(init: { fixtures?: ReturnType<typeof libraryFixtures> | null; suggestions?: Suggestions | "untrusted"; platform?: "stored" | "planned"; delayMs?: number; limits?: Partial<LibraryLimits> } = {}): LibraryMock {
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
  const summary = (): LibrarySummary => ({ workflows: workflows.size, prompts: prompts.size, env_sets: sets.size, keychain: init.platform === "planned" ? "unavailable" : "ok", limits: { ...PINNED_LIMITS, ...init.limits } });
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
      limit(workflows, init.limits?.workflows ?? PINNED_LIMITS.workflows);
      const one: Workflow = { ...draft, name: draft.name.trim(), id: nextId("01jwn"), ...local, ...stamp };
      workflows.set(one.id, one); writes.push(`workflow.create:${one.name}`); return one;
    },
    async updateWorkflow(id, draft, seen) {
      await wait(); preflight();
      const one = workflows.get(id) ?? refuse("not_found");
      if (seen !== one.revision) refuse("revision_conflict");
      if (taken(workflows, draft.name, id)) refuse("name_taken");
      // An edit to the command or the parameters drops the recorded ack of a non-local entry; a rename does not. The source stays.
      const changed = draft.command !== one.command || JSON.stringify(draft.params) !== JSON.stringify(one.params);
      const next = bump({ ...one, ...draft, name: draft.name.trim(), needs_ack: one.source !== "local" && (one.needs_ack || changed) });
      workflows.set(id, next); writes.push(`workflow.update:${next.name}`); return next;
    },
    async deleteWorkflow(id) { await wait(); preflight(); workflows.delete(id); writes.push(`workflow.delete:${id}`); },
    async runWorkflow(id, values, where, ackGrants = []) {
      await wait(); preflight();
      const one = workflows.get(id) ?? refuse("not_found");
      fill(one.command, one.params, values);
      const attached = one.env_set_ids.flatMap((setId) => { const set = sets.get(setId); return set ? [set] : []; });
      const needed: ProfileGrant[] = [...(attached.length > 0 ? ["env_values" as const] : []), ...(one.source !== "local" && one.needs_ack ? ["untrusted_source" as const] : [])];
      // The ack must match exactly what the run needs: a missing code and an extra one are both `confirm_required`.
      if (needed.length !== ackGrants.length || needed.some((grant) => !ackGrants.includes(grant))) {
        const [status, message] = STATUS.confirm_required!;
        throw new ApiFailure("confirm_required", message, status, { grants: needed, env_sets: attached.map((set) => ({ name: set.name, keys: set.keys.map((key) => key.name) })) });
      }
      if (needed.includes("untrusted_source")) workflows.set(id, { ...one, needs_ack: false });
      writes.push(`workflow.run:${id}:${"thread_id" in where ? `thread=${where.thread_id}` : `project=${where.project_id}`}${ackGrants.length > 0 ? `:ack=${[...ackGrants].join("+")}` : ""}`);
      return { terminal_id: "term-1" };
    },
    async createPrompt(draft: PromptDraft) {
      await wait(); preflight();
      if (taken(prompts, draft.name, null)) refuse("name_taken");
      limit(prompts, init.limits?.prompts ?? PINNED_LIMITS.prompts);
      const one: Prompt = { ...draft, name: draft.name.trim(), id: nextId("01jpn"), source: "local", ...stamp };
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
      limit(sets, init.limits?.env_sets ?? PINNED_LIMITS.env_sets);
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
    async importLibrary(document, dryRun): Promise<ImportAnswer> {
      await wait(); preflight();
      const body = typeof document === "object" && document !== null ? (document as Record<string, unknown>) : {};
      const lines: ImportAnswer["lines"] = [];
      let created = 0; let renamed = 0; let refused = 0;
      for (const [kind, key] of [["workflow", "workflows"], ["prompt", "prompts"], ["env_set", "env_sets"]] as const) {
        const list = Array.isArray(body[key]) ? (body[key] as { name?: unknown }[]) : [];
        list.forEach((entry, index) => {
          const name = typeof entry?.name === "string" ? entry.name.trim() : "";
          if (name === "") { refused += 1; lines.push({ kind, index, outcome: "refused", code: "invalid_input" }); return; }
          const all = kind === "workflow" ? workflows : kind === "prompt" ? prompts : sets;
          const clash = taken(all as Map<string, { id: string; name: string }>, name, null);
          if (clash) renamed += 1; else created += 1;
          lines.push({ kind, index, outcome: clash ? "renamed" : "created", name: clash ? `${name} (2)` : name });
        });
      }
      if (!dryRun) writes.push(`import:${created}+${renamed}`);
      return { dry_run: dryRun, problems: [], created, renamed, refused, lines };
    },
    async suggestions() {
      await wait();
      if (init.suggestions === "untrusted") return refuse("folder_untrusted");
      return init.suggestions ?? suggestionFixtures();
    },
    async copySuggestion(_projectId, file, index): Promise<Copied> {
      await wait(); preflight();
      const found = init.suggestions && init.suggestions !== "untrusted" ? init.suggestions : suggestionFixtures();
      // The entry's own file and index name it; an index counts dropped entries too, so it is not a position in the served array.
      if (file === "workflows.json") { const one = found.workflows.find((entry) => entry.file === file && entry.index === index) ?? refuse("not_found"); const created = await this.createWorkflow({ name: one.name, description: one.description, command: one.command, params: one.params, env_set_ids: one.env_set_names.flatMap((name) => [...sets.values()].filter((set) => set.name === name).map((set) => set.id)) }); const entry: Workflow = { ...created, source: "repo", needs_ack: true }; workflows.set(entry.id, entry); return { kind: "workflow", entry }; }
      if (file === "prompts.json") { const one = found.prompts.find((entry) => entry.file === file && entry.index === index) ?? refuse("not_found"); const created = await this.createPrompt({ name: one.name, description: one.description, body: one.body, params: one.params }); const entry: Prompt = { ...created, source: "repo" }; prompts.set(entry.id, entry); return { kind: "prompt", entry }; }
      const one = found.env_sets.find((entry) => entry.file === file && entry.index === index) ?? refuse("not_found");
      return { kind: "env_set", entry: await this.createEnvSet({ name: one.name, description: one.description, keys: one.keys }) };
    },
  };
}
