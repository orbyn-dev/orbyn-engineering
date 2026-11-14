import { useCallback, useEffect, useMemo, useState } from "react";
import type { ApiFailure } from "../../../api/client";
import { useAsync } from "../../../lib/useAsync";
import { crewApi } from "../autopilot/approvals/api";
import { useHeldCommands, type HoldControl } from "../autopilot/approvals/held";
import type { ApprovalRow } from "../autopilot/approvals/model";
import { backgroundItems, heldItems, mainItems, rank, stopItems } from "./assemble";
import { needsApi } from "./api";
import type { NeedsItem } from "./model";

/** How often the inbox reads its routes again while the window is shown. Held commands refresh on their own timer. */
export const NEEDS_REFRESH_MS = 30_000;

export type NeedsState =
  | { status: "loading" }
  | { status: "error"; failure: ApiFailure; retry(): void }
  | { status: "ready"; items: NeedsItem[]; hold(id: string): { control: HoldControl; readable: boolean } | null; approve(id: string, nonce: string): Promise<boolean>; confirming: ApprovalRow | null; ask(id: string): void; closeAsk(): void; partial: string[] };

/**
 * The inbox from the routes that exist. Mount it only while Crew Preview is on (`NeedsYouInbox` does): it reads at once. One source failing does not hide the others: the ones that read are listed, and the ones
 * that did not are named in `partial`. All four failing is an error.
 */
export function useNeedsYou(): NeedsState {
  const [confirming, setConfirming] = useState<ApprovalRow | null>(null);
  const held = useHeldCommands(() => setConfirming(null));
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
      ...(stops.data ? stopItems(stops.data.recent, { inARow: stops.data.in_a_row_limit, inARun: stops.data.in_a_run_limit }) : []),
      ...(review.data ? mainItems(review.data) : []),
      ...(background.data ? backgroundItems(background.data) : []),
    ]);
  }, [heldRows, stops.data, review.data, background.data]);

  const ask = useCallback((id: string) => setConfirming(heldRows.find((row) => row.id === id) ?? null), [heldRows]);
  const closeAsk = useCallback(() => setConfirming(null), []);
  const hold = useCallback((id: string) => {
    const row = heldRows.find((one) => one.id === id);
    return row ? { control: held.control(row, () => setConfirming(row)), readable: row.held?.readable ?? false } : null;
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
  return { status: "ready", items, hold, approve: held.approve, confirming, ask, closeAsk, partial: failed.map((one) => one.name) };
}
