import { useMemo, useState } from "react";
import { navigate } from "../../../product/state/route";
import { ErrorState, Loading } from "../../../product/ui/states";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { ApproveOnceDialog } from "../autopilot/approvals/HeldCard";
import { HELD } from "../autopilot/approvals/copy";
import { UNAVAILABLE_REASON } from "../autopilot/review/Parts";
import { viewOf, only } from "./assemble";
import type { NeedsAction, NeedsItem, NeedsKind } from "./model";
import { kindChips, NeedsBar, NeedsEmpty, NeedsList, RANKED_BY, useNarrow, type NeedsActions } from "./NeedsYou";
import { useNeedsYou } from "./useNeedsYou";

const UNREADABLE = "This command is too long to read whole, so it cannot be approved once.";

/**
 * The Needs you inbox on live data. Nothing is drawn while Crew Preview is off. Answers go only through the held-command client
 * that already exists (approve once after "Confirm it's you", deny); Open goes to the screen that holds the item. Nothing here
 * merges, reverts or approves anything else.
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
      if (action.key === "approve") state.ask(item.ref);
      else if (action.key === "deny") state.hold(item.ref)?.control.deny();
      else if (action.key === "open") navigate({ name: "crew", tab: item.kind === "review" ? "background" : "approvals" });
    },
    off(item, action) {
      // A break on main has no screen in this app yet to open.
      if (action.key === "open") return item.kind === "main" ? UNAVAILABLE_REASON : null;
      const hold = state.hold(item.ref);
      if (!hold) return UNAVAILABLE_REASON;
      const { control, readable } = hold;
      if (control.busy) return "Waiting for the answer";
      if (control.remote) return HELD.remote;
      if (action.key === "approve") return control.standing !== "signed_in" ? HELD.noAccount : readable ? null : UNREADABLE;
      return null;
    },
  };
  const errors = items.flatMap((item) => {
    const message = item.kind === "approval" ? state.hold(item.ref)?.control.error : null;
    return message ? [{ id: item.id, message }] : [];
  });

  return (
    <section ref={ref} className="ny-root" aria-label="Needs you">
      {state.partial.length ? <p className="ny-note" role="status">Can’t read {state.partial.join(", ")}. The rest is shown.</p> : null}
      {items.length === 0 ? (
        <NeedsEmpty title="Nothing needs you">When something needs you, it appears here, ranked, with what waiting costs.</NeedsEmpty>
      ) : (
        <>
          <NeedsBar<NeedsKind | "all"> chips={kindChips(view, items.length)} active={view.filters.some((one) => one.kind === filter) ? filter : "all"} onPick={setFilter} rule={RANKED_BY} />
          <NeedsList items={shown} narrow={narrow} handlers={handlers} />
          {errors.map((one) => <p key={one.id} className="ny-note" role="alert">{one.message}</p>)}
        </>
      )}
      {state.confirming ? <ApproveOnceDialog row={state.confirming} approve={state.approve} onClose={state.closeAsk} /> : null}
    </section>
  );
}
