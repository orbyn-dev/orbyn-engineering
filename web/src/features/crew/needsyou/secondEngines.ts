import { useCallback, useState } from "react";
import { ApiFailure } from "../../../api/client";
import type { AutopilotSecondEngineNotice } from "../../../generated/AutopilotSecondEngineNotice";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { crewApi } from "../autopilot/approvals/api";

/** Cursor paging over pending notices, not a bounded scan of recent denials.
 * Exceeding the read bound is a visible source failure, never a complete list. */
export async function readSecondEngineNotices(signal?: AbortSignal): Promise<AutopilotSecondEngineNotice[]> {
  const items: AutopilotSecondEngineNotice[] = [];
  let before: number | null = null;
  for (let page = 0; page < 500; page += 1) {
    const next = await crewApi.secondEngineNotices(before, signal);
    items.push(...next.items);
    if (next.next_before === null) return items;
    if (next.next_before <= 0 || (before !== null && next.next_before >= before)) break;
    before = next.next_before;
  }
  throw new ApiFailure("notice_paging", "Can’t read second-engine notices.", 0);
}

/** The service owns persistence and idempotency. Dismiss hides only this notice;
 * neither Dismiss nor visiting Providers retries or approves the denied action. */
export function useSecondEngineNotices() {
  const read = useAsync("crew-second-engine-notices", readSecondEngineNotices, { refreshOn: (what) => what === "engines" || what.startsWith("models:") });
  const [failure, setFailure] = useState<ApiFailure | null>(null);
  const reloadRead = read.reload;
  const reload = useCallback(() => { setFailure(null); return reloadRead(); }, [reloadRead]);
  const dismiss = useCallback((id: number) => {
    setFailure(null);
    void crewApi.dismissSecondEngineNotice(id).then(() => reloadRead()).catch((error: unknown) => setFailure(toFailure(error)));
  }, [reloadRead]);
  return { ...read, reload, dismiss, failure };
}
