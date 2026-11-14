import { useMemo, useState } from "react";
import { navigate } from "../../../product/state/route";
import { ErrorState, Loading } from "../../../product/ui/states";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { HELD } from "../autopilot/approvals/copy";
import { UNAVAILABLE_REASON } from "../autopilot/review/Parts";
import { viewOf, only } from "./assemble";
import type { NeedsAction, NeedsItem, NeedsKind } from "./model";
import { kindChips, NeedsBar, NeedsEmpty, NeedsList, CHIP_NOTE, useNarrow, type NeedsActions } from "./NeedsYou";
import { useNeedsYou } from "./useNeedsYou";

/**
 * The Needs you inbox on live data. Nothing is drawn while Crew Preview is off. Deny goes through the held-command client that
 * already exists. Approve and Open go to the Approvals tab, where the approve-once flow lives; the inbox has no approve flow of its
 * own. Nothing here merges, reverts or approves anything.
 */
export function NeedsYouInbox() {
  // A service with Crew Preview off answers 404, and so does one with no Crew routes: both read as off, and no other route is read.
  return useCrewPreview() ? <Inbox /> : null;
}

function Inbox() {
  const state = useNeedsYou();
  const [filter, setFilter] = useState<NeedsKind | "all">("all");
  const [ref, narrow] = useNarrow();
  const ready = state.status === "ready" ? state : null;
  const items = useMemo(() => ready?.items ?? [], [ready]);
  const view = useMemo(() => viewOf(items), [items]);
  const shown = only(items, view.filters.some((one) => one.kind === filter) ? filter : "all");

  if (state.status === "loading") return <Loading label="Reading what needs you" rows={4} />;
  if (state.status === "error") return <ErrorState title="Can't read what needs you" failure={state.failure} onRetry={state.retry} />;

  const handlers: NeedsActions = {
    act(item: NeedsItem, action: NeedsAction) {
      if (action.key === "dismiss") state.dismiss(item.id);
      else if (action.key === "deny") state.hold(item.ref)?.deny();
      else if (action.key === "approve" || action.key === "open") navigate({ name: "crew", tab: item.kind === "background" ? "background" : "approvals" });
    },
    off(item, action) {
      // A break on main has no screen in this app yet to open.
      if (action.key !== "deny") return item.kind === "main" ? UNAVAILABLE_REASON : null;
      const control = state.hold(item.ref);
      if (!control) return UNAVAILABLE_REASON;
      if (control.busy) return "Waiting for the answer";
      if (control.closed) return control.closed === "expired" ? HELD.expired : HELD.decided;
      return control.remote ? HELD.remote : null;
    },
  };
  const errors = items.flatMap((item) => {
    const message = item.kind === "approval" ? state.hold(item.ref)?.error : null;
    return message ? [{ id: item.id, message }] : [];
  });

  return (
    <section ref={ref} className="ny-root" aria-label="Needs you">
      {items.length === 0 && state.partial.length === 0 ? (
        <NeedsEmpty title="Nothing needs you">When something needs you, it appears here.</NeedsEmpty>
      ) : (
        <>
          <NeedsBar<NeedsKind | "all"> chips={kindChips(view, items.length)} active={view.filters.some((one) => one.kind === filter) ? filter : "all"} onPick={setFilter} rule={CHIP_NOTE} />
          <NeedsList items={shown} narrow={narrow} handlers={handlers} errors={state.partial} onRetry={state.retry} />
          {errors.map((one) => <p key={one.id} className="ny-note" role="alert">{one.message}</p>)}
        </>
      )}
    </section>
  );
}
