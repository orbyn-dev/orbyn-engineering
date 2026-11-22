/**
 * What the limits label says on every screen that shows the figures (Usage, the /usage-limits panel, the thread
 * Limits row): the age of the last good answer in the app's own words, "Checking…" while a person's refresh runs, and
 * the sentence for a failed refresh. The age comes from the shared store; the shared clock keeps it counting.
 */
import { agoWords } from "./format";
import { useClock } from "./clock";
import { limitsAnsweredAt } from "./limitsStore";

export interface LimitsChecked {
  /** A person's refresh is on its way and the earlier figures are still shown. */
  checking: boolean;
  /** The last refresh failed; the figures shown are the last good answer. */
  failed: boolean;
  /** "Checked 5 min ago", "Checking…", or null when there is no answer yet. */
  label: string | null;
  /** "Couldn’t check limits. Showing the answer from 12 min ago." */
  failure: string | null;
}

export function useLimitsChecked(key: string, state: { status: string; data: unknown; refreshing: boolean }): LimitsChecked {
  const now = useClock();
  const at = limitsAnsweredAt(key);
  const shown = state.data !== null && state.data !== undefined;
  const checking = shown && state.refreshing && state.status !== "loading";
  const failed = shown && !checking && state.status === "error";
  const age = at === null ? null : agoWords(at, Math.max(now, at));
  return {
    checking,
    failed,
    label: checking ? "Checking…" : age === null || !shown ? null : `Checked ${age}`,
    failure: failed ? (age === null ? "Couldn’t check limits." : `Couldn’t check limits. Showing the answer from ${age}.`) : null,
  };
}
