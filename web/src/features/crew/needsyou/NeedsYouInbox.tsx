import { useMemo, useState } from "react";
import { navigate } from "../../../product/state/route";
import { ErrorState, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { useAreaOverride } from "../autopilot/approvals/areaOverride";
import { useWatchFocus } from "../autopilot/approvals/watchFocus";
import { useRepo } from "../autopilot/approvals/AutonomyAreas";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { HELD } from "../autopilot/approvals/copy";
import { UNAVAILABLE_REASON } from "../autopilot/review/Parts";
import { runWrite, useWrites } from "../writes/store";
import { ConfirmWrite, WriteStatus, type Ask } from "../writes/TaskWrite";
import { useConnection } from "../../../product/state/connection";
import { viewOf, only } from "./assemble";
import type { NeedsAction, NeedsItem, NeedsKind } from "./model";
import { kindChips, NeedsBar, NeedsEmpty, NeedsList, CHIP_NOTE, useNarrow, type NeedsActions } from "./NeedsYou";
import { useNeedsYou } from "./useNeedsYou";

/**
 * The Needs you inbox on live data. Nothing is drawn while Crew Preview is off. Deny goes through the held-command client that
 * already exists. Approve and Open go to the Approvals tab, where the approve-once flow lives; the inbox has no approve flow of
 * its own. A demoted area's Review opens Autopilot by area and its Override… opens the page's own menu; both stay person-only.
 * Nothing here merges, reverts or approves anything.
 */
export function NeedsYouInbox() {
  // A service with Crew Preview off answers 404, and so does one with no Crew routes: both read as off, and no other route is read.
  return useCrewPreview() ? <Inbox /> : null;
}

function Inbox() {
  // The same repository the Background tab reads: the first git project, or the one in the address.
  const repo = useRepo();
  const project = repo === "loading" || repo === null ? null : repo.id;
  const state = useNeedsYou(project);
  const override = useAreaOverride({ project, reload: () => { if (state.status === "ready") void state.reloadAreas(); } });
  const [filter, setFilter] = useState<NeedsKind | "all">("all");
  const [ref, narrow] = useNarrow();
  const [asking, setAsking] = useState<{ id: string; name: string; kind: Ask } | null>(null);
  const ops = useWrites((writes) => writes.ops);
  const remote = useConnection((connection) => connection.remote);
  const ready = state.status === "ready" ? state : null;
  const items = useMemo(() => ready?.items ?? [], [ready]);
  const view = useMemo(() => viewOf(items), [items]);
  const shown = only(items, view.filters.some((one) => one.kind === filter) ? filter : "all");

  if (state.status === "loading") return <Loading label="Reading what needs you" rows={4} />;
  if (state.status === "error") return <ErrorState title="Can’t read what needs you" failure={state.failure} onRetry={state.retry} />;

  const handlers: NeedsActions = {
    act(item: NeedsItem, action: NeedsAction, anchor: HTMLElement) {
      if (item.kind === "could_not_start") setAsking({ id: item.ref, name: item.from, kind: action.key === "retry" ? "retry" : "dismiss" });
      else if (item.kind === "autonomy") {
        // The demotion's own actions (needsyou-demoted): the row's Review opens the area, Override… the page's menu, Dismiss is remembered here.
        if (action.key === "dismiss") state.dismiss(item.id);
        else if (action.key === "review") navigate({ name: "settings", section: "crew", crewPage: "areas", ...(project ? { repo: project } : {}) });
        else if (action.key === "override") {
          const row = state.areas.find((one) => one.area_key === item.ref);
          if (row) override.open(row, anchor);
        }
      }
      else if (action.key === "dismiss") state.dismiss(item.id);
      else if (action.key === "sign-in") navigate({ name: "settings", section: "providers" });
      else if (action.key === "deny") state.hold(item.ref)?.deny();
      else if (action.key === "approve" || action.key === "open") {
        // A watch row opens that watch's own page on the Background tab; a background run opens the list it waits on.
        if (item.id.startsWith("watch:")) useWatchFocus.getState().ask(item.ref, item.project);
        navigate({ name: "crew", tab: item.kind === "background" ? "background" : "approvals" });
      }
    },
    off(item, action) {
      if (item.kind === "could_not_start") {
        const op = ops[item.ref];
        if (remote) return "Not available from another computer";
        if (op?.phase === "busy") return "Waiting for the answer";
        // The service said no other engine can take it: Retry stays off and says why; Dismiss still works.
        if (action.key === "retry" && op?.kind === "retry" && op.phase === "failed" && op.code === "task_busy") return op.message;
        return null;
      }
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
        <NeedsEmpty title="Nothing needs you" action={<Button small onClick={() => navigate({ name: "crew", tab: "flow" })}>Open the Flow view</Button>}>
          When something needs you, it appears here.
        </NeedsEmpty>
      ) : (
        <>
          <NeedsBar<NeedsKind | "all"> chips={kindChips(view, items.length)} active={view.filters.some((one) => one.kind === filter) ? filter : "all"} onPick={setFilter} rule={CHIP_NOTE} />
          <NeedsList items={shown} narrow={narrow} handlers={handlers} errors={state.partial} onRetry={state.retry} />
          {items.filter((item) => item.kind === "could_not_start").map((item) => <WriteStatus key={item.id} id={item.ref} op={ops[item.ref]} />)}
          {errors.map((one) => <p key={one.id} className="ny-note" role="alert">{one.message}</p>)}
          {state.noticeFailure ? <p className="ny-note" role="alert">{state.noticeFailure.message}</p> : null}
        </>
      )}
      {repo !== "loading" && repo?.several ? <p className="ny-note">Autonomy shows {repo.name} only.</p> : null}
      {asking ? (
        <ConfirmWrite
          kind={asking.kind}
          name={asking.name}
          onClose={() => setAsking(null)}
          onConfirm={(reason) => {
            setAsking(null);
            void runWrite(asking.kind === "retry" ? "retry" : "cancel", asking.id, reason, state.retry);
          }}
        />
      ) : null}
      {override.node}
    </section>
  );
}
