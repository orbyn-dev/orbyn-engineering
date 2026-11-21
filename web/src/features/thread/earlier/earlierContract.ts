/**
 * The messages an imported thread had before it was imported (parity K36), built hidden. An import resumes the agent's own session and
 * copies no messages, so core has nothing to read them from yet; `earlierApi()` answers `null` in a production build and the thread
 * opens as it does today. The route is written in docs/attachments-transcripts.md; the mock lives in web/mock/earlier.ts.
 */

export interface EarlierMessage {
  role: "user" | "assistant" | "tool";
  /** Plain text. Never markup: the screen draws it as text. */
  text: string;
}

export interface EarlierPage {
  /** The provider the history came from, such as `Claude Code`. */
  source: string;
  /** How many messages the history holds in all. */
  total: number;
  imported_at: string;
  /** The newest messages first read are the last of `messages`; this is the cursor for the ones before them, or `null` at the start. */
  next_before: string | null;
  /** Oldest first. */
  messages: EarlierMessage[];
}

export interface EarlierApi {
  /**
   * The last page, or the page before `before`. Resolves `null` for a thread that was not imported. Rejects with `history_unreadable`
   * when the provider's history has moved, changed or is too new.
   */
  read(threadId: string, page: { before?: string }, signal: AbortSignal): Promise<EarlierPage | null>;
}

let current: EarlierApi | null = null;

export const earlierApi = (): EarlierApi | null => current;

/** Tests and the dev-only preview install a client here; `null` puts back the production default, which sends nothing. */
export function setEarlierApi(next: EarlierApi | null): void {
  current = next;
}

if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/earlier")) {
  void import("../../../../mock/earlier").then((module) => setEarlierApi(module.createEarlierMock({ delayMs: 400 })));
}
