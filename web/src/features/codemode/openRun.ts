import { navigate, parseRoute } from "../../product/state/route";
import { runAnchorId } from "./model";
import { scrollTimelineToRun } from "./timelineScroll";

const TRIES = 30;

const reducedMotion = (): boolean => {
  try {
    return window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true;
  } catch {
    return false;
  }
};

/**
 * Opens a run in its thread: goes to the thread (the thread's own `#/threads/<id>` route; nothing new in the address), asks the mounted
 * timeline to scroll to the run's row by its place in the list (a long thread mounts only the rows near the view), then looks for the
 * run's block, `id="cm-run-<run id>"`, over the next animation frames because a screen change draws a moment later. When it is there it
 * is scrolled into view (no smooth scroll under reduced motion) and focused. `done(false)` when it never shows; nothing else happens then.
 * Display only: no write, no decision. Returns a function that stops the search.
 */
export function openRun(threadId: string, codeRun: string, done: (found: boolean) => void): () => void {
  const id = runAnchorId(codeRun);
  if (!id) {
    done(false);
    return () => undefined;
  }
  const here = parseRoute(window.location.hash);
  if (!(here.name === "thread" && here.id === threadId)) navigate({ name: "thread", id: threadId });
  let tries = 0;
  let frame = 0;
  let stopped = false;
  // The timeline may mount after the navigation, and its rows after the log reads; ask again until one has the run.
  let asked = false;
  const look = () => {
    if (stopped) return;
    if (!asked) asked = scrollTimelineToRun(threadId, codeRun);
    const element = document.getElementById(id);
    if (element) {
      element.scrollIntoView?.({ block: "center", behavior: reducedMotion() ? "auto" : "smooth" });
      element.focus({ preventScroll: true });
      done(true);
      return;
    }
    tries += 1;
    if (tries >= TRIES) {
      done(false);
      return;
    }
    frame = requestAnimationFrame(look);
  };
  frame = requestAnimationFrame(look);
  return () => {
    stopped = true;
    cancelAnimationFrame(frame);
  };
}
