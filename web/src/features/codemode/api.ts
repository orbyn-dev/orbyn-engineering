// The one door the Code Mode screens use to reach the service. Named operations live here; no other file in the feature
// calls the transport.
//
// MOCK ONLY, UNTIL THE BACKEND LANDS. docs/code-mode.md names `code_run.get(runId)` and the per-engine `code_mode` setting, but
// neither route is on main yet (no crates/orbyn-contracts type, no orbynd route, no web/src/generated file). The paths below
// are this client's own guess at them, served only by web/mock/codemode.ts. `codeModeLive` is the build-time preview flag:
// false in a production build, where nothing here sends a request and the screens say Planned instead. When the real routes
// are merged, swap the three functions for the generated types and delete the flag.
import { request } from "../../api/client";
import type { CodeRunRecord, CodeRunSummary } from "./types";

const q = encodeURIComponent;

/** True only in a dev or test build, where the mock serves the routes. Never true in a production build. */
export const codeModeLive: boolean = __ORBYN_PREVIEWS__;

/** `code_run.get(runId)`: one run record, limited by the service to the caller's own project. */
export function getCodeRun(runId: string, signal?: AbortSignal) {
  return request<CodeRunRecord>("GET", `/api/code-runs/${q(runId)}`, undefined, signal);
}

/** The engine's `code_mode` setting. Off is the default. */
export function getCodeMode(engineId: string, signal?: AbortSignal) {
  return request<{ enabled: boolean }>("GET", `/api/engines/${q(engineId)}/code-mode`, undefined, signal);
}

/** One write per click: the new value, and the service answers with what it stored. */
export function putCodeMode(engineId: string, enabled: boolean) {
  return request<{ enabled: boolean }>("PUT", `/api/engines/${q(engineId)}/code-mode`, { enabled });
}

/** The runs of one thread, newest first, limited by the service to the caller's own project. GUESSED path, mock only. */
export function listCodeRuns(threadId: string, signal?: AbortSignal) {
  return request<{ runs: CodeRunSummary[] }>("GET", `/api/threads/${q(threadId)}/code-runs`, undefined, signal);
}
