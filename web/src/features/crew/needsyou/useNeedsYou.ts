import { useCallback, useEffect, useMemo, useState } from "react";
import type { ApiFailure } from "../../../api/client";
import { serviceUrl } from "../../../api/origin";
import { useAsync } from "../../../lib/useAsync";
import { crewApi } from "../autopilot/approvals/api";
import { useHeldCommands, type HoldControl } from "../autopilot/approvals/held";
import { backgroundItems, couldNotStartItems, heldItems, mainItems, rank, secondEngineItems, stopItems } from "./assemble";
import { useSecondEngineNotices } from "./secondEngines";
import { needsApi } from "./api";
import type { NeedsItem } from "./model";

/** How often the inbox reads its routes again while the window is shown. Held commands refresh on their own timer. */
export const NEEDS_REFRESH_MS = 30_000;

/**
 * "Stop answered", for the beta, is client-side only: no route says a trip-wire stop was dealt with, so a stop the person dismisses is
 * remembered in this browser's local storage and hidden here. It changes nothing on the service and is not shared with other
 * windows or devices. A new route would replace this.
 */
const DISMISSED_KEY = "orbyn.needsyou.dismissedStops";

/** The service this window reaches: a stop's key is scoped to it, so another service, or a remote host, never hides this one's stops. */
const scope = () => {
  try {
    return new URL(serviceUrl("/"), window.location.href).origin;
  } catch {
    return window.location.origin;
  }
};
// The scope is the service origin. A desktop service that restarts on another port is another origin, so its dismissed stops come back:
// that fails safe (a stop shows again, none is hidden). Pruning drops the old entries on the next load.
const dismissKey = (id: string) => `${scope()}|${id}`;

function readDismissed(): Set<string> {
  try {
    const list: unknown = JSON.parse(window.localStorage.getItem(DISMISSED_KEY) ?? "[]");
    return new Set(Array.isArray(list) ? list.filter((one): one is string => typeof one === "string") : []);
  } catch {
    return new Set();
  }
}

export type NeedsState =
  | { status: "loading" }
  | { status: "error"; failure: ApiFailure; retry(): void }
  | { status: "ready"; items: NeedsItem[]; hold(id: string): HoldControl | null; partial: string[]; noticeFailure: ApiFailure | null; retry(): void; dismiss(id: string): void };

/**
 * The inbox from the routes that exist. Mount it only while Crew Preview is on (`NeedsYouInbox` does): it reads at once. One source failing does not hide the others: the ones that read are listed, and the ones
 * that did not are named in `partial`. All sources failing is an error.
 */
export function useNeedsYou(): NeedsState {
  // Approving a held command is the Approvals tab's own flow; the inbox only denies (and opens).
  const [dismissed, setDismissed] = useState(readDismissed);
  const held = useHeldCommands(() => undefined);
  const stops = useAsync("needs-stops", (signal) => crewApi.stops(signal));
  const review = useAsync("needs-review", (signal) => needsApi.reviewQueue(signal));
  const background = useAsync("needs-background", (signal) => needsApi.backgroundWaiting(signal));
  const refused = useAsync("needs-could-not-start", (signal) => needsApi.couldNotStart(signal));
  const notices = useSecondEngineNotices();
  const reloadNotices = notices.reload;
  const quietNotices = notices.refreshQuietly;
  const quietStops = stops.refreshQuietly;
  const quietReview = review.refreshQuietly;
  const quietBackground = background.refreshQuietly;
  const quietRefused = refused.refreshQuietly;
  const reloadStops = stops.reload;
  const reloadReview = review.reload;
  const reloadBackground = background.reload;
  const reloadRefused = refused.reload;
  const reloadHeld = held.reload;

  useEffect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState === "hidden") return;
      // A read the timer starts keeps the list on screen and never turns a source into an error: only a person's retry does.
      void quietStops();
      void quietReview();
      void quietBackground();
      void quietNotices();
      void quietRefused();
    }, NEEDS_REFRESH_MS);
    return () => clearInterval(timer);
  }, [quietStops, quietReview, quietBackground, quietNotices, quietRefused]);

  const retry = useCallback(() => {
    void reloadHeld();
    void reloadStops();
    void reloadReview();
    void reloadBackground();
    void reloadNotices();
    void reloadRefused();
  }, [reloadHeld, reloadStops, reloadReview, reloadBackground, reloadNotices, reloadRefused]);

  const heldRows = held.rows;
  const items = useMemo(() => {
    const now = Date.now();
    return rank([
      ...heldItems(heldRows, now),
      ...(stops.data ? stopItems(stops.data.recent, { inARow: stops.data.in_a_row_limit, inARun: stops.data.in_a_run_limit }).filter((one) => !dismissed.has(dismissKey(one.id))) : []),
      ...(refused.data ? couldNotStartItems(refused.data) : []),
      ...(review.data ? mainItems(review.data) : []),
      ...(background.data ? backgroundItems(background.data) : []),
      ...(notices.data ? secondEngineItems(notices.data) : []),
    ]);
  }, [heldRows, stops.data, review.data, background.data, refused.data, notices.data, dismissed]);
  // Keep only the dismissals whose stop is still in the service's list, so the stored list cannot grow without end.
  const recent = stops.data?.recent;
  useEffect(() => {
    if (!recent) return;
    const present = new Set(
      stopItems(recent, { inARow: 0, inARun: 0 }).map((one) => dismissKey(one.id)),
    );
    setDismissed((now) => {
      const kept = [...now].filter((key) => present.has(key));
      if (kept.length === now.size) return now;
      try {
        window.localStorage.setItem(DISMISSED_KEY, JSON.stringify(kept));
      } catch {
        // Storage is off: nothing to prune.
      }
      return new Set(kept);
    });
  }, [recent]);
  const dismissNotice = notices.dismiss;
  const dismiss = useCallback((id: string) => {
    if (id.startsWith("second-engine:")) { dismissNotice(Number(id.slice("second-engine:".length))); return; }
    setDismissed((now) => {
      const next = new Set(now).add(dismissKey(id));
      try {
        window.localStorage.setItem(DISMISSED_KEY, JSON.stringify([...next]));
      } catch {
        // Storage is off: the stop is hidden until the window is closed.
      }
      return next;
    });
  }, [dismissNotice]);

  const hold = useCallback((id: string) => {
    const row = heldRows.find((one) => one.id === id);
    return row ? held.control(row) : null;
  }, [heldRows, held]);

  const reads = [
    { name: "held commands", failed: held.failure !== null, error: held.failure },
    { name: "stops", failed: stops.status === "error", error: stops.error },
    { name: "the Review queue", failed: review.status === "error", error: review.error },
    { name: "background crews", failed: background.status === "error", error: background.error },
    { name: "tasks that could not start", failed: refused.status === "error", error: refused.error },
    { name: "second-engine notices", failed: notices.status === "error", error: notices.error },
  ];
  const failed = reads.filter((one) => one.failed);
  // The held read has no loading flag of its own: the inbox waits for the other sources, and held commands join when they arrive.
  const loading = stops.status === "loading" && review.status === "loading" && background.status === "loading" && refused.status === "loading" && notices.status === "loading";
  if (failed.length === reads.length) return { status: "error", failure: failed[0]!.error as ApiFailure, retry };
  if (loading) return { status: "loading" };
  return { status: "ready", items, hold, partial: failed.map((one) => one.name), noticeFailure: notices.failure, retry, dismiss };
}
