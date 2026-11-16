import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { create } from "zustand";
import type { FlowReviewConsent } from "../../../../generated/FlowReviewConsent";
import type { CrewReviewConsentChoice } from "../../../../generated/CrewReviewConsentChoice";
import { useConnection } from "../../../../product/state/connection";
import { navigate } from "../../../../product/state/route";
import { toFailure } from "../../../../lib/useAsync";
import { Button } from "../../../../ui/controls";
import { Dialog } from "../../../../ui/Dialog";
import { flowApi } from "./api";

type Write = { phase: "busy" } | { phase: "done"; choice: CrewReviewConsentChoice; started: boolean } | { phase: "failed"; message: string };
// The invocation identity includes the full stored scope, so navigation/remounts
// cannot submit the same request twice or attach its receipt to another task.
export const consentKey = (c: FlowReviewConsent) => [c.task, c.id, c.lane, c.round, c.commit].join("|");
const useConsentWrites = create<{ writes: Record<string, Write> }>(() => ({ writes: {} }));

function record(key: string, write: Write | null) {
  useConsentWrites.setState((s) => {
    const writes = { ...s.writes };
    if (write) writes[key] = write; else delete writes[key];
    // Keep in-flight requests even when an older task is no longer mounted.
    for (const [old, value] of Object.entries(writes)) {
      if (Object.keys(writes).length <= 128) break;
      if (old !== key && value.phase !== "busy") delete writes[old];
    }
    return { writes };
  });
}

async function answer(target: FlowReviewConsent, choice: CrewReviewConsentChoice, changed: () => void) {
  const key = consentKey(target);
  if (useConnection.getState().remote || !target.can_answer || useConsentWrites.getState().writes[key]?.phase === "busy") return;
  record(key, { phase: "busy" });
  try {
    const receipt = await flowApi.consent(target.task, { request: target.id, choice });
    if (receipt.subject !== target.task || receipt.request !== target.id) throw new Error("The review answer belongs to another request.");
    record(key, { phase: "done", choice, started: receipt.started });
    changed();
  } catch (error) {
    record(key, { phase: "failed", message: toFailure(error).message });
  }
}

/** Approved consent drawer on the live stage list; no standing preference. */
export function ReviewConsent({ consent, onChanged }: { consent: FlowReviewConsent; onChanged(): void }) {
  const key = consentKey(consent);
  const write = useConsentWrites((s) => s.writes[key]);
  const remote = useConnection((s) => s.remote);
  const [open, setOpen] = useState(false);
  const mounted = useRef(true);
  const liveKey = useRef(key);
  useLayoutEffect(() => { liveKey.current = key; }, [key]);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const busy = write?.phase === "busy";
  const unresolved = write?.phase === "failed";
  const answeredOnce = write?.phase === "done" && write.choice === "once";
  const enabled = !remote && consent.can_answer && !busy && !unresolved && !answeredOnce;
  const changed = () => { if (mounted.current && liveKey.current === key) onChanged(); };
  const reconcile = async () => {
    try {
      const fresh = await flowApi.task(consent.task);
      const pending = fresh.review_consents?.find((c) => consentKey(c) === key);
      if (pending?.can_answer) record(key, null);
      changed();
    } catch (error) { record(key, { phase: "failed", message: toFailure(error).message }); }
  };
  return (
    <li className="fv-review-pending">
      <button type="button" className="fv-review-open" aria-label={`${consent.lane}: Needs you`} onClick={() => setOpen(true)}>
        <b>{consent.lane}</b><span className="fv-st fv-st-warn">Needs you</span>
      </button>
      {write?.phase === "done" ? <p role="status">{write.started ? "Same-engine review started." : "The task stays here. Stages before this one keep their results."}</p> : null}
      {open ? <Dialog className="fv-review-drawer" title={`${consent.lane} can’t start on a different engine`} busy={busy} onClose={() => setOpen(false)} footer={<>
        <Button small data-dialog-focus disabled={!enabled} onClick={() => void answer({ ...consent }, "once", changed)}>Run on {consent.engine} once</Button>
        <Button small tone="primary" disabled={busy} onClick={() => navigate({ name: "settings", section: "providers" })}>Sign in to another engine</Button>
        <Button small disabled={!enabled} onClick={() => void answer({ ...consent }, "wait", changed)}>Wait</Button>
        <Button small tone="quiet" disabled={busy} onClick={() => setOpen(false)}>Close</Button>
      </>}>
        <span className="fv-st fv-st-warn">Needs you</span><span className="fv-badge">Lead tier required</span>
        <div className="fv-review-warning"><p>{consent.lane} needs a different engine at the lead tier, and none is available. Nothing started.</p></div>
        <dl className="fv-review-facts"><dt>Engine</dt><dd>{consent.engine} · {consent.model}</dd><dt>Round</dt><dd>{consent.round}</dd><dt>Checkout</dt><dd>Read-only at <code>{consent.commit}</code></dd></dl>
        <h3>Your options</h3>
        <section><h4>Run on {consent.engine} once</h4><p>Its own separate run, labelled “Same engine as author”.</p></section>
        <section><h4>Sign in to another engine</h4></section>
        <section><h4>Wait</h4><p>The task stays here. Stages before this one keep their results.</p></section>
        {remote ? <p>Not available from another computer</p> : null}
        {consent.state === "expired" ? <p role="status">This review consent has expired.</p> : null}
        {busy ? <p role="status">Waiting for the answer</p> : null}
        {write?.phase === "failed" ? <div role="alert"><p>{write.message}</p><Button small onClick={() => void reconcile()}>Reload task</Button></div> : null}
      </Dialog> : null}
    </li>
  );
}
