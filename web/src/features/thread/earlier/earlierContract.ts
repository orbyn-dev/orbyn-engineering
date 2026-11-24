/**
 * The messages an imported thread had before it was imported (parity K36). The read is Local and answers `null` for a thread that
 * was not imported, so the screen draws nothing; every other failure reaches the screen, which keeps the thread usable. The route is
 * written in docs/attachments-transcripts.md; the mock lives in web/mock/earlier.ts and is installed only by the dev preview.
 */
import { ApiFailure, request } from "../../../api/client";
import type { EarlierMessage } from "../../../generated/EarlierMessage";
import type { EarlierPage } from "../../../generated/EarlierPage";

export type { EarlierMessage, EarlierPage };

export interface EarlierApi {
  /**
   * The last page, or the page before `before`. Resolves `null` for a thread that was not imported. Rejects with `history_unreadable`
   * when the provider's history has moved, changed or is too new, and with `bad_cursor` when the page cursor is stale.
   */
  read(threadId: string, page: { before?: string }, signal: AbortSignal): Promise<EarlierPage | null>;
}

const real: EarlierApi = {
  async read(threadId, page, signal) {
    const before = page.before === undefined ? "" : `?before=${encodeURIComponent(page.before)}`;
    try {
      return await request<EarlierPage>(
        "GET",
        `/api/threads/${encodeURIComponent(threadId)}/earlier${before}`,
        undefined,
        signal,
      );
    } catch (failure) {
      // Not imported, or a service that does not have this route yet: both mean there is nothing to draw.
      if (failure instanceof ApiFailure && failure.status === 404) return null;
      throw failure;
    }
  },
};

let current: EarlierApi = real;

export const earlierApi = (): EarlierApi => current;

/** Tests and the dev-only preview install a client here; `null` puts the real client back. */
export function setEarlierApi(next: EarlierApi | null): void {
  current = next ?? real;
}

if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/earlier")) {
  void import("../../../../mock/earlier").then((module) => setEarlierApi(module.createEarlierMock({ delayMs: 400 })));
}
