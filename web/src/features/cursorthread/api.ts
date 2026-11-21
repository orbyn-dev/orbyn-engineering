import { useEffect, useState } from "react";
import { useAsync } from "../../lib/useAsync";
import type { CursorThreadApi, NoticeState } from "./contract";

/**
 * The real client for the notice and override routes. Those routes are not in the service's route table yet, so this one makes no
 * request at all: the notice reads as "none", and a write says the service does not offer it. Swap it for `request(...)` calls the
 * day they land: `GET /api/projects/{project_id}/engines/{engine}/notice`, `POST .../notice/acknowledge` and
 * `POST /api/threads/{id}/send-without-approvals` (the two POSTs are person routes: their paths are already in
 * `PENDING_PERSON_ROUTES` in `api/personKey.ts`); the screens do not change.
 */
const unreported: CursorThreadApi = {
  notice: async () => null,
  acknowledge: async () => { throw new Error("This service does not offer that yet."); },
  sendWithoutApprovals: async () => { throw new Error("This service does not offer that yet."); },
};

let current: CursorThreadApi = unreported;
let version = 0;
const listeners = new Set<() => void>();

// Dev only: `#/preview/cursor-thread` installs the mock so the screens can be seen before the routes exist. In a production build
// the flag is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/cursor-thread")) {
  void import("../../../mock/cursorThread").then((module) => setCursorThreadApi(module.createCursorThreadMock({ delayMs: 300 })));
}

export const cursorThreadApi = (): CursorThreadApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the unreported client back. */
export function setCursorThreadApi(next: CursorThreadApi | null): void {
  current = next ?? unreported;
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
