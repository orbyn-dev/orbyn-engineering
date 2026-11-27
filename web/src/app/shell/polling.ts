import { useWorkspace } from "../../product/state/workspace";
import { POLL_MS, quickFocus, quickTick } from "../pollSchedule";
import { lastFull, loadAll } from "./loading";

/**
 * Whether this tick reads: a hidden window reads nothing, because nothing it draws has changed and
 * coming back reads at once. Work still running in it keeps reading, or no alert would ever fire.
 */
export const shouldPoll = (hidden: boolean, running: boolean): boolean => !hidden || running;

/**
 * The shell's poll: projects, the one thread list and approvals every tick, and the Git-backed
 * status far less often (see `pollSchedule`). A window that becomes visible reads once at once —
 * what it could not see while hidden — and a window that hides stops reading entirely.
 *
 * A hidden window therefore does not look in for work another client started: it learns of such a
 * run at that one read when it is shown, and then reads on while the run lasts, because a run it
 * knows is running keeps this poll going. The notices that must fire while hidden keep their own
 * polls and are not this one's: a scheduled task's run (`useTaskRunNotifications`) and a Crew item
 * that needs the person (`useCrewNotifications`).
 */
export function startShellPolling(): () => void {
  let ticks = 0;
  const timer = setInterval(() => {
    ticks += 1;
    const running = Object.values(useWorkspace.getState().threads).some((thread) => thread.active_run);
    if (shouldPoll(document.hidden, running)) void loadAll(quickTick(ticks));
  }, POLL_MS);
  const onVisibility = () => {
    if (!document.hidden) void loadAll(quickFocus(Date.now(), lastFull));
  };
  document.addEventListener("visibilitychange", onVisibility);
  return () => {
    clearInterval(timer);
    document.removeEventListener("visibilitychange", onVisibility);
  };
}
