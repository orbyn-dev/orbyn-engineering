/** The shell's poll reads projects, threads and approvals every tick; Git-backed status is read far less often. */
export const POLL_MS = 4000;
/** Branches and update status come from running Git over every worktree: read them every few minutes, and on focus. */
export const SLOW_POLL_EVERY = 45;
/** Coming back to the window reads them again, but not more than once in this long. */
export const FOCUS_FULL_AFTER_MS = 60_000;

/** True when this tick leaves the Git-backed status alone. */
export const quickTick = (tick: number): boolean => tick % SLOW_POLL_EVERY !== 0;

/** True when a focus right now should leave the Git-backed status alone: it was read a moment ago. */
export const quickFocus = (now: number, lastFull: number): boolean => now - lastFull < FOCUS_FULL_AFTER_MS;
