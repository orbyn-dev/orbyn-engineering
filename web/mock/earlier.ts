/**
 * MOCK of the earlier-messages route of docs/attachments-transcripts.md (provisional), for tests and the dev-only preview
 * (`#/preview/earlier`), which installs it through `setEarlierApi`. The app's own default is no client at all: nothing here ships.
 * It answers a history for any thread, the way the Engineer frames `pc-k36-earlier` and `pc-k36-unreadable` show one. Put
 * `?unreadable` in the hash, as in `#/preview/earlier?unreadable`, to see the history that cannot be read.
 */
import { ApiFailure } from "../src/api/client.ts";
import type { EarlierApi, EarlierMessage, EarlierPage } from "../src/features/thread/earlier/earlierContract.ts";

const HISTORY: EarlierMessage[] = [
  { role: "user", text: "The logout page redirects back to itself in a loop." },
  { role: "tool", text: "Read 3 files" },
  { role: "assistant", text: "The cookie is cleared after the redirect has already fired, so the next request still looks signed in. I would clear it first." },
  { role: "user", text: "Do that, then check the sign-in page does the same." },
  { role: "tool", text: "Edited auth/logout.ts +4 −1" },
];

export function createEarlierMock(init: { delayMs?: number; unreadable?: boolean } = {}): EarlierApi & { reads: Array<string | undefined> } {
  const reads: Array<string | undefined> = [];
  const unreadable = init.unreadable ?? (typeof location !== "undefined" && location.hash.includes("unreadable"));
  return {
    reads,
    async read(_threadId, page, signal): Promise<EarlierPage> {
      reads.push(page.before);
      await new Promise((resolve) => setTimeout(resolve, init.delayMs ?? 0));
      if (signal.aborted) throw new DOMException("Aborted", "AbortError");
      if (unreadable) throw new ApiFailure("history_unreadable", "The session file has moved.", 422);
      const older = page.before !== undefined;
      return {
        source: "Claude Code",
        total: 312,
        imported_at: "2026-11-21T10:00:00Z",
        next_before: older ? null : "cursor-1",
        messages: older ? [{ role: "user", text: "Earlier still: the first report of the loop." }] : HISTORY,
      };
    },
  };
}
