import { useEffect, useState } from "react";
import { ApiFailure, request } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import type { CursorThreadApi, NoticeState } from "./contract";

/**
 * The real client for the notice and override routes (docs/cursor-thread-contract.md). The notice read is Local and answers 404 for an
 * engine with no notice, which reads as `null`; the acknowledgement and the one-turn override are Person-class, so `request` sends
 * the person key with them (their paths are in `PERSON_ROUTES` in `api/personKey.ts`). The override body is the normal run body
 * with no permission mode: the service refuses one, since this runs a single turn and saves nothing.
 */
const real: CursorThreadApi = {
  notice: async (projectId, engine, signal) => {
    try {
      return await request<NoticeState>("GET", `/api/projects/${encodeURIComponent(projectId)}/engines/${encodeURIComponent(engine)}/notice`, undefined, signal);
    } catch (failure) {
      if (failure instanceof ApiFailure && failure.status === 404) return null;
      throw failure;
    }
  },
  acknowledge: (projectId, engine) =>
    request<NoticeState>("POST", `/api/projects/${encodeURIComponent(projectId)}/engines/${encodeURIComponent(engine)}/notice/acknowledge`, {}),
  sendWithoutApprovals: (threadId, body) =>
    request<unknown>("POST", `/api/threads/${encodeURIComponent(threadId)}/send-without-approvals`, { prompt: body.prompt, attachment_ids: body.attachment_ids ?? [] }),
};

let current: CursorThreadApi = real;
let version = 0;
const listeners = new Set<() => void>();

// Dev only: `#/preview/cursor-thread` installs the mock so the screens can be seen before the routes exist. In a production build
// the flag is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/cursor-thread")) {
  void import("../../../mock/cursorThread").then((module) => setCursorThreadApi(module.createCursorThreadMock({ delayMs: 300 })));
}

export const cursorThreadApi = (): CursorThreadApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the real client back. */
export function setCursorThreadApi(next: CursorThreadApi | null): void {
  current = next ?? real;
  version += 1;
  for (const listener of listeners) listener();
}

function useApiVersion(): number {
  const [seen, setSeen] = useState(version);
  useEffect(() => {
    const listener = () => setSeen(version);
    listeners.add(listener);
    listener();
    return () => { listeners.delete(listener); };
  }, []);
  return seen;
}

/** Whether the project has answered the engine's notice; `null` while unread or when the service has no notice for this engine. */
export function useEngineNotice(projectId: string, engine: string, enabled = true): NoticeState | null {
  const seen = useApiVersion();
  const on = enabled && projectId !== "";
  const read = useAsync(`engine-notice:${projectId}:${engine}:${on}:${seen}`, (signal) => (on ? cursorThreadApi().notice(projectId, engine, signal) : Promise.resolve(null)));
  return read.status === "ready" ? read.data : null;
}
