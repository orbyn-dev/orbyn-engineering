import { useCallback, useEffect, useMemo, useState } from "react";
import type { ApiFailure } from "../../../api/client";
import { useAsync } from "../../../lib/useAsync";
import { crewApi } from "../autopilot/approvals/api";
import { useHeldCommands, type HoldControl } from "../autopilot/approvals/held";
import { backgroundItems, heldItems, mainItems, rank, stopItems } from "./assemble";
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
  | { status: "ready"; items: NeedsItem[]; hold(id: string): HoldControl | null; partial: string[]; retry(): void; dismiss(id: string): void };

/**
 * The inbox from the routes that exist. Mount it only while Crew Preview is on (`NeedsYouInbox` does): it reads at once. One source failing does not hide the others: the ones that read are listed, and the ones
 * that did not are named in `partial`. All four failing is an error.
 */
export function useNeedsYou(): NeedsState {
  // Approving a held command is the Approvals tab's own flow; the inbox only denies (and opens).
  const [dismissed, setDismissed] = useState(readDismissed);
  const held = useHeldCommands(() => undefined);
  const stops = useAsync("needs-stops", (signal) => crewApi.stops(signal));
  const review = useAsync("needs-review", (signal) => needsApi.reviewQueue(signal));
  const background = useAsync("needs-background", (signal) => needsApi.backgroundWaiting(signal));
  const reloadStops = stops.reload;
  const reloadReview = review.reload;
  const reloadBackground = background.reload;
  const reloadHeld = held.reload;

  useEffect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState === "hidden") return;
      void reloadStops();
      void reloadReview();
      void reloadBackground();
    }, NEEDS_REFRESH_MS);
    return () => clearInterval(timer);
  }, [reloadStops, reloadReview, reloadBackground]);

  const retry = useCallback(() => {
    void reloadHeld();
    void reloadStops();
    void reloadReview();
    void reloadBackground();
  }, [reloadHeld, reloadStops, reloadReview, reloadBackground]);

  const heldRows = held.rows;
  const items = useMemo(() => {
    const now = Date.now();
    return rank([
      ...heldItems(heldRows, now),
      ...(stops.data ? stopItems(stops.data.recent, { inARow: stops.data.in_a_row_limit, inARun: stops.data.in_a_run_limit }).filter((one) => !dismissed.has(one.id)) : []),
      ...(review.data ? mainItems(review.data) : []),
      ...(background.data ? backgroundItems(background.data) : []),
    ]);
  }, [heldRows, stops.data, review.data, background.data, dismissed]);
  const dismiss = useCallback((id: string) => {
    setDismissed((now) => {
      const next = new Set(now).add(id);
      try {
        window.localStorage.setItem(DISMISSED_KEY, JSON.stringify([...next]));
      } catch {
        // Storage is off: the stop is hidden until the window is closed.
      }
      return next;
    });
  }, []);

  const hold = useCallback((id: string) => {
    const row = heldRows.find((one) => one.id === id);
    return row ? held.control(row, () => undefined) : null;
  }, [heldRows, held]);

  const reads = [
    { name: "held commands", failed: held.failure !== null, error: held.failure },
    { name: "stops", failed: stops.status === "error", error: stops.error },
    { name: "the Review queue", failed: review.status === "error", error: review.error },
    { name: "background crews", failed: background.status === "error", error: background.error },
  ];
  const failed = reads.filter((one) => one.failed);
  // The held read has no loading flag of its own: the inbox waits for the other three, and held commands join when they arrive.
  const loading = stops.status === "loading" && review.status === "loading" && background.status === "loading";
  if (failed.length === reads.length) return { status: "error", failure: failed[0]!.error as ApiFailure, retry };
  if (loading) return { status: "loading" };
  return { status: "ready", items, hold, partial: failed.map((one) => one.name), retry, dismiss };
}
