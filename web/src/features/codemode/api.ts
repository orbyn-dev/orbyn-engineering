// The one door the Code Mode screens use to reach the service. Named operations live here; no other file in the feature
// calls the transport.
//
// The routes are the service's own (docs/code-mode.md, "As built: run record, routes and the switch"). They exist only when the
// service runs with Code Mode on (`ORBYN_CODE_MODE=1`); with it off every one of them answers 404. A 404 is therefore "Code Mode
// is not offered" and is answered as `null`, never thrown: the screens keep drawing the Planned or hidden state and show no error.
// For a run record `null` also covers a run that is gone (records are kept 7 days) or is not this project's.
import { ApiFailure, request } from "../../api/client";
import type { CodeModeSetting, CodeRunList, CodeRunRecord, CodeRunSummary } from "./types";

const q = encodeURIComponent;
const isMissing = (error: unknown) => error instanceof ApiFailure && error.status === 404;

/** A read or write that treats `404` as "not offered". Every other failure is thrown as it is. */
async function orNull<T>(send: Promise<T>): Promise<T | null> {
  try {
    return await send;
  } catch (error) {
    if (isMissing(error)) return null;
    throw error;
  }
}

/** `GET /api/code-runs/{id}`: one run record. `null` when there is none, or Code Mode is not offered. */
export function getCodeRun(runId: string, signal?: AbortSignal): Promise<CodeRunRecord | null> {
  return orNull(request<CodeRunRecord>("GET", `/api/code-runs/${q(runId)}`, undefined, signal));
}

/**
 * `POST /api/code-runs/{id}/cancel` (a Person route, no body): ends a live run and answers the updated record.
 * `null` when there is none, or Code Mode is not offered.
 */
export function cancelCodeRun(runId: string): Promise<CodeRunRecord | null> {
  return orNull(request<CodeRunRecord>("POST", `/api/code-runs/${q(runId)}/cancel`));
}

/** `GET /api/engines/{id}/code-mode`: the engine's switch (off is the default). `null` when Code Mode is not offered. */
export function getCodeMode(engineId: string, signal?: AbortSignal): Promise<CodeModeSetting | null> {
  return orNull(request<CodeModeSetting>("GET", `/api/engines/${q(engineId)}/code-mode`, undefined, signal));
}

/** `PUT /api/engines/{id}/code-mode` (a Person route): one write per click, answered with what the service stored. `null` when not offered. */
export function putCodeMode(engineId: string, enabled: boolean): Promise<CodeModeSetting | null> {
  return orNull(request<CodeModeSetting>("PUT", `/api/engines/${q(engineId)}/code-mode`, { enabled }));
}

/** `GET /api/threads/{id}/code-runs`: the runs of one thread, newest first (at most 100). `null` when Code Mode is not offered. */
export async function listCodeRuns(threadId: string, signal?: AbortSignal): Promise<CodeRunSummary[] | null> {
  const list = await orNull(request<CodeRunList>("GET", `/api/threads/${q(threadId)}/code-runs`, undefined, signal));
  return list === null ? null : Array.isArray(list.runs) ? list.runs : [];
}
