import { useCallback, useEffect, useMemo, useState } from "react";
import type { ApiFailure } from "../../../api/client";
import { serviceUrl } from "../../../api/origin";
import { useAsync } from "../../../lib/useAsync";
import { crewApi } from "../autopilot/approvals/api";
import { useHeldCommands, type HoldControl } from "../autopilot/approvals/held";
import type { AutopilotAreaRow } from "../../../generated/AutopilotAreaRow";
import type { AutopilotAreas } from "../../../generated/AutopilotAreas";
import { autonomyItems, backgroundItems, couldNotStartItems, couldNotStopItems, heldItems, mainItems, rank, secondEngineItems, stopItems, watchItems } from "./assemble";
import { useSecondEngineNotices } from "./secondEngines";
import { needsApi } from "./api";
import type { NeedsItem } from "./model";

/** How often the inbox reads its routes again while the window is shown. Held commands refresh on their own timer. */
export const NEEDS_REFRESH_MS = 30_000;

/** No repository to read the areas of: the read resolves empty, so a repo-less service is not a failed source. */
const NO_AREAS: AutopilotAreas = { project: "", ceiling: "full", as_of: 0, enforced: true, total: 0, rows: [] };

/**
 * "Stop answered" and "Autonomy row dismissed", for the beta, are client-side only: no route says a trip-wire stop was dealt with or
 * that a person has seen a demotion, so a dismissal is remembered in this browser's local storage and hidden here. It changes nothing
 * on the service and is not shared with other windows or devices. New routes would replace this.
 */
const DISMISSED_KEY = "orbyn.needsyou.dismissedStops";
const DISMISSED_AUTONOMY_KEY = "orbyn.needsyou.dismissedAutonomy";

/** The service this window reaches: an id is scoped to it, so another service, or a remote host, never hides this one's rows. */
const scope = () => {
  try {
    return new URL(serviceUrl("/"), window.location.href).origin;
  } catch {
    return window.location.origin;
  }
};
// The scope is the service origin. A desktop service that restarts on another port is another origin, so its dismissed rows come back:
// that fails safe (a row shows again, none is hidden). Pruning drops the old entries on the next load.
const dismissKey = (id: string) => `${scope()}|${id}`;

function readDismissed(key: string): Set<string> {
  try {
    const list: unknown = JSON.parse(window.localStorage.getItem(key) ?? "[]");
    return new Set(Array.isArray(list) ? list.filter((one): one is string => typeof one === "string") : []);
  } catch {
    return new Set();
  }
}

function writeDismissed(key: string, ids: ReadonlySet<string>): void {
  try {
    window.localStorage.setItem(key, JSON.stringify([...ids]));
  } catch {
    // Storage is off: the row is hidden until the window is closed.
  }
}

export type NeedsState =
  | { status: "loading" }
  | { status: "error"; failure: ApiFailure; retry(): void }
  | { status: "ready"; items: NeedsItem[]; hold(id: string): HoldControl | null; partial: string[]; noticeFailure: ApiFailure | null; retry(): void; reloadAreas(): void; dismiss(id: string): void; areas: AutopilotAreaRow[] };

/**
 * The inbox from the routes that exist. Mount it only while Crew Preview is on (`NeedsYouInbox` does): it reads at once. One source failing does not hide the others: the ones that read are listed, and the ones
 * that did not are named in `partial`. All sources failing is an error. The areas are read for `project` when there is one (the same
 * first git project the Background tab reads); the demoted rows of any other repository are not in the inbox until a repo-scoped inbox
 * exists (needsyou-many).
 */
export function useNeedsYou(project: string | null): NeedsState {
  // Approving a held command is the Approvals tab's own flow; the inbox only denies (and opens).
  const [dismissed, setDismissed] = useState(() => readDismissed(DISMISSED_KEY));
  const [dismissedAutonomy, setDismissedAutonomy] = useState(() => readDismissed(DISMISSED_AUTONOMY_KEY));
  const held = useHeldCommands(() => undefined);
  const stops = useAsync("needs-stops", (signal) => crewApi.stops(signal));
  const review = useAsync("needs-review", (signal) => needsApi.reviewQueue(signal));
  const background = useAsync("needs-background", (signal) => needsApi.backgroundWaiting(signal));
  const shared = useAsync("needs-shared", (signal) => needsApi.sharedWaiting(signal));
  const refused = useAsync("needs-could-not-start", (signal) => needsApi.couldNotStart(signal));
  const areas = useAsync(`needs-areas:${project ?? "none"}`, (signal) => (project ? crewApi.areas(project, signal) : Promise.resolve(NO_AREAS)), { keep: true });
  const notices = useSecondEngineNotices();
  const reloadNotices = notices.reload;
  const quietNotices = notices.refreshQuietly;
  const quietStops = stops.refreshQuietly;
  const quietReview = review.refreshQuietly;
  const quietBackground = background.refreshQuietly;
  const quietShared = shared.refreshQuietly;
  const quietRefused = refused.refreshQuietly;
  const quietAreas = areas.refreshQuietly;
  const reloadStops = stops.reload;
  const reloadReview = review.reload;
  const reloadBackground = background.reload;
  const reloadShared = shared.reload;
  const reloadRefused = refused.reload;
  const reloadAreas = areas.reload;
  const reloadHeld = held.reload;

  useEffect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState === "hidden") return;
      // A read the timer starts keeps the list on screen and never turns a source into an error: only a person's retry does.
      void quietStops();
      void quietReview();
      void quietBackground();
      void quietShared();
      void quietNotices();
      void quietRefused();
      void quietAreas();
    }, NEEDS_REFRESH_MS);
    return () => clearInterval(timer);
  }, [quietStops, quietReview, quietBackground, quietShared, quietNotices, quietRefused, quietAreas]);

  const retry = useCallback(() => {
    void reloadHeld();
    void reloadStops();
    void reloadReview();
    void reloadBackground();
    void reloadShared();
    void reloadNotices();
    void reloadRefused();
    void reloadAreas();
  }, [reloadHeld, reloadStops, reloadReview, reloadBackground, reloadShared, reloadNotices, reloadRefused, reloadAreas]);

  const heldRows = held.rows;
  const areaRows = areas.data?.rows;
  const items = useMemo(() => {
    const now = Date.now();
    return rank([
      ...heldItems(heldRows, now),
      ...(stops.data ? stopItems(stops.data.recent, { inARow: stops.data.in_a_row_limit, inARun: stops.data.in_a_run_limit }).filter((one) => !dismissed.has(dismissKey(one.id))) : []),
      ...(refused.data ? couldNotStartItems(refused.data) : []),
      ...(shared.data ? couldNotStopItems(shared.data.stopped) : []),
      ...(review.data ? mainItems(review.data) : []),
      ...(background.data ? backgroundItems(background.data) : []),
      ...(shared.data ? watchItems(shared.data.watch) : []),
      ...(areaRows && project ? autonomyItems(areaRows, project).filter((one) => !dismissedAutonomy.has(dismissKey(one.id))) : []),
      ...(notices.data ? secondEngineItems(notices.data) : []),
    ]);
  }, [heldRows, stops.data, review.data, background.data, shared.data, refused.data, notices.data, areaRows, project, dismissed, dismissedAutonomy]);
  // Keep only the dismissals whose row is still in the service's list, so the stored lists cannot grow without end.
  const recent = stops.data?.recent;
  useEffect(() => {
    if (!recent) return;
    const present = new Set(
      stopItems(recent, { inARow: 0, inARun: 0 }).map((one) => dismissKey(one.id)),
    );
    setDismissed((now) => {
      const kept = [...now].filter((key) => present.has(key));
      if (kept.length === now.size) return now;
      writeDismissed(DISMISSED_KEY, new Set(kept));
      return new Set(kept);
    });
  }, [recent]);
  // A dismissed demotion returns only if the area trips again (a new trip time is a new row). Only this repository's own entries are
  // pruned: another repository's stay until it is shown again.
  useEffect(() => {
    if (!project || !areaRows) return;
    const own = dismissKey(`autonomy:${project}:`);
    const present = new Set(autonomyItems(areaRows, project).map((one) => dismissKey(one.id)));
    setDismissedAutonomy((now) => {
      const kept = [...now].filter((key) => !key.startsWith(own) || present.has(key));
      if (kept.length === now.size) return now;
      writeDismissed(DISMISSED_AUTONOMY_KEY, new Set(kept));
      return new Set(kept);
    });
  }, [project, areaRows]);
  const dismissNotice = notices.dismiss;
  const dismiss = useCallback((id: string) => {
    if (id.startsWith("second-engine:")) { dismissNotice(Number(id.slice("second-engine:".length))); return; }
    if (id.startsWith("autonomy:")) {
      setDismissedAutonomy((now) => {
        const next = new Set(now).add(dismissKey(id));
        writeDismissed(DISMISSED_AUTONOMY_KEY, next);
        return next;
      });
      return;
    }
    setDismissed((now) => {
      const next = new Set(now).add(dismissKey(id));
      writeDismissed(DISMISSED_KEY, next);
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
    { name: "watches and tasks that could not be stopped", failed: shared.status === "error", error: shared.error },
    { name: "tasks that could not start", failed: refused.status === "error", error: refused.error },
    // Without a repository there is no areas read: it is not a source that can fail, so it never keeps a failing service looking readable.
    ...(project ? [{ name: "Autopilot areas", failed: areas.status === "error", error: areas.error }] : []),
    { name: "second-engine notices", failed: notices.status === "error", error: notices.error },
  ];
  const failed = reads.filter((one) => one.failed);
  // The held read has no loading flag of its own: the inbox waits for the other sources, and held commands join when they arrive.
  // Without a repository there is no areas read to wait for.
  const loading = stops.status === "loading" && review.status === "loading" && background.status === "loading" && shared.status === "loading" && refused.status === "loading" && notices.status === "loading" && (project === null || areas.status === "loading");
  if (failed.length === reads.length) return { status: "error", failure: failed[0]!.error as ApiFailure, retry };
  if (loading) return { status: "loading" };
  return { status: "ready", items, hold, partial: failed.map((one) => one.name), noticeFailure: notices.failure, retry, reloadAreas: quietAreas, dismiss, areas: areaRows ?? [] };
}
