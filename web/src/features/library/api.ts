import { useEffect, useState } from "react";
import { ApiFailure, request } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import { EXPORT_KINDS } from "./contract";
import type { Copied, EnvSet, LibraryApi, LibraryDocument, LibrarySummary, Prompt, Suggestions, Workflow } from "./contract";

const q = encodeURIComponent;
const isMissing = (error: unknown) => error instanceof ApiFailure && error.status === 404;
const isObject = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
/** A list answer: an array of objects. Anything else is a service this page does not know, not a crash. */
const items = <T,>(value: unknown): T[] => (Array.isArray(value) ? (value.filter(isObject) as unknown as T[]) : []);

/**
 * The real client for the routes of docs/execution-profiles-and-library.md, Part 2 (crates/orbynd/src/routes/library.rs, checked
 * path by path; not run against a live service). `GET /api/library` answering `404` is "an older service has no library" and is answered as `null` with no error, so
 * nothing draws. Every write is Person-class (`personKey.ts`): `request` adds the key. Ids in a path are encoded and only ever ones
 * a list gave. An environment value goes only in the body of `PUT .../keys/{key}`: never in a path, a query or a log.
 */
const http: LibraryApi = {
  async summary(signal) {
    try {
      const summary = await request<LibrarySummary>("GET", "/api/library", undefined, signal);
      if (!isObject(summary) || typeof summary.workflows !== "number") return null;
      return summary;
    } catch (error) {
      if (isMissing(error)) return null;
      throw error;
    }
  },
  workflows: async (signal) => items<Workflow>(await request<unknown>("GET", "/api/library/workflows", undefined, signal)),
  prompts: async (signal) => items<Prompt>(await request<unknown>("GET", "/api/library/prompts", undefined, signal)),
  envSets: async (signal) => items<EnvSet>(await request<unknown>("GET", "/api/library/env-sets", undefined, signal)),
  renderWorkflow: (id, values, signal) => request("POST", `/api/library/workflows/${q(id)}/render`, { values }, signal),
  renderPrompt: (id, values, signal) => request("POST", `/api/library/prompts/${q(id)}/render`, { values }, signal),
  createWorkflow: (draft) => request("POST", "/api/library/workflows", draft),
  updateWorkflow: (id, draft, seen) => request("PATCH", `/api/library/workflows/${q(id)}`, { ...draft, seen_revision: seen }),
  deleteWorkflow: async (id) => { await request<unknown>("DELETE", `/api/library/workflows/${q(id)}`); },
  // `ack_grants` is exactly what a `409 confirm_required` named (core compares the sets, an extra code is refused too). The generated
  // type lists it as required, so it is always sent; `[]` for a run that needs none.
  runWorkflow: async (id, values, where, ackGrants = []) => {
    const answer = await request<unknown>("POST", `/api/library/workflows/${q(id)}/run`, { values, ...where, ack_grants: [...ackGrants] });
    if (!isObject(answer) || typeof answer.terminal_id !== "string") throw new ApiFailure("bad_answer", "The service answered a run in a shape this app does not know.", 502);
    return { terminal_id: answer.terminal_id };
  },
  createPrompt: (draft) => request("POST", "/api/library/prompts", draft),
  updatePrompt: (id, draft, seen) => request("PATCH", `/api/library/prompts/${q(id)}`, { ...draft, seen_revision: seen }),
  deletePrompt: async (id) => { await request<unknown>("DELETE", `/api/library/prompts/${q(id)}`); },
  createEnvSet: (draft) => request("POST", "/api/library/env-sets", draft),
  updateEnvSet: (id, draft, seen) => request("PATCH", `/api/library/env-sets/${q(id)}`, { ...draft, seen_revision: seen }),
  deleteEnvSet: async (id) => { await request<unknown>("DELETE", `/api/library/env-sets/${q(id)}`); },
  setKey: (setId, key, value) => request("PUT", `/api/library/env-sets/${q(setId)}/keys/${q(key)}`, { value }),
  removeKey: (setId, key) => request("DELETE", `/api/library/env-sets/${q(setId)}/keys/${q(key)}`),
  async suggestions(projectId, signal) {
    const answer = await request<unknown>("GET", `/api/projects/${q(projectId)}/library/suggestions`, undefined, signal);
    const body = isObject(answer) ? answer : {};
    return { workflows: items(body.workflows), prompts: items(body.prompts), env_sets: items(body.env_sets), problems: items(body.problems) } as Suggestions;
  },
  copySuggestion: (projectId, file, index) => request<Copied>("POST", `/api/projects/${q(projectId)}/library/suggestions/copy`, { file, index }),
  // The document is the body (the service reads bytes and refuses over `import_bytes`); `dry_run` is a top-level field beside `format`.
  importLibrary: (document, dryRun) => request("POST", "/api/library/import", { ...(isObject(document) ? document : {}), dry_run: dryRun }),
  // Every kind is asked for explicitly, so the file a person saves holds all four even if the service's default ever narrows.
  async exportLibrary(kinds = EXPORT_KINDS, signal) {
    const query = kinds.length > 0 ? `?kinds=${q(kinds.join(","))}` : "";
    const document = await request<LibraryDocument>("GET", `/api/library/export${query}`, undefined, signal);
    if (!isObject(document) || typeof document.format !== "string") throw new ApiFailure("bad_answer", "The service answered an export in a shape this app does not know.", 502);
    return document;
  },
};

let current: LibraryApi = http;
let version = 0;
/** Whether the service has the feature: read once per client, so the panel strip asks the service one time. */
let probe: Promise<boolean> | null = null;
const listeners = new Set<() => void>();

// Dev only: `#/preview/library` installs the mock so the panel can be seen before the routes exist. In a production build the flag
// is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/library")) {
  void import("../../../mock/library").then((module) => setLibraryApi(module.createLibraryMock({ delayMs: 300 })));
}

export const libraryApi = (): LibraryApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the real client back. */
export function setLibraryApi(next: LibraryApi | null): void {
  current = next ?? http;
  probe = null;
  version += 1;
  for (const listener of listeners) listener();
}

/** Changes when the client is swapped, so a screen reads again from the new one. */
export function useApiVersion(): number {
  const [seen, setSeen] = useState(version);
  useEffect(() => {
    const listener = () => setSeen(version);
    listeners.add(listener);
    listener();
    return () => { listeners.delete(listener); };
  }, []);
  return seen;
}

/** `true` once the service answered the summary; `false` while asking, on `404`, and on any failure (the feature stays hidden, no error). */
export function useLibraryOffered(): boolean {
  const seen = useApiVersion();
  const [offered, setOffered] = useState(false);
  useEffect(() => {
    let live = true;
    probe ??= libraryApi().summary().then((summary) => summary !== null, () => false);
    void probe.then((answer) => { if (live) setOffered(answer); });
    return () => { live = false; };
  }, [seen]);
  return offered;
}

export interface LibraryData { summary: LibrarySummary; workflows: Workflow[]; prompts: Prompt[]; envSets: EnvSet[] }

/** The three lists and the counts, read together. The read is latest-wins (`useAsync`): a slow earlier answer never replaces a later one. */
export function useLibrary(): { data: LibraryData | null; error: ApiFailure | null; settled: boolean; reload(): void } {
  const seen = useApiVersion();
  const read = useAsync(`library:${seen}`, async (signal): Promise<LibraryData | null> => {
    const summary = await libraryApi().summary(signal);
    if (summary === null) return null;
    const [workflows, prompts, envSets] = await Promise.all([libraryApi().workflows(signal), libraryApi().prompts(signal), libraryApi().envSets(signal)]);
    return { summary, workflows, prompts, envSets };
  });
  return {
    data: read.status === "ready" ? read.data : null,
    error: read.status === "error" ? read.error : null,
    settled: read.status !== "loading",
    reload: read.reload,
  };
}

/** The repository's own entries for a project, read-only. `null` while loading. A `403 folder_untrusted` is reported as `untrusted`, not as a failure. */
export function useSuggestions(projectId: string): { data: Suggestions | null; untrusted: boolean; error: ApiFailure | null; reload(): void } {
  const seen = useApiVersion();
  const read = useAsync(`library-suggestions:${seen}:${projectId}`, async (signal): Promise<Suggestions | "untrusted"> => {
    try {
      return await libraryApi().suggestions(projectId, signal);
    } catch (error) {
      if (error instanceof ApiFailure && error.code === "folder_untrusted") return "untrusted";
      throw error;
    }
  });
  return {
    data: read.status === "ready" && read.data !== "untrusted" ? read.data : null,
    untrusted: read.status === "ready" && read.data === "untrusted",
    error: read.status === "error" ? read.error : null,
    reload: read.reload,
  };
}
