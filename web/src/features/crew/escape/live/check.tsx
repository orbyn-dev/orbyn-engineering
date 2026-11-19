import { useState, type ReactNode } from "react";
import type { EscapeCheckRow } from "../../../../generated/EscapeCheckRow";
import type { EscapeRow } from "../../../../generated/EscapeRow";
import { Planned } from "../../capacity/Planned";
import { Btn } from "../../shared/parts";
import { checkStateLabel, WHY } from "./model";

export const short = (commit: string) => commit.slice(0, 7);
const mono = { fontFamily: "var(--font-mono)", fontSize: "11px", wordBreak: "break-all", lineHeight: 1.5 } as const;

/** The design draws seven hex of a commit id; the whole id stays one hover away. */
export const Commit = ({ id }: { id: string }) => <span style={mono} title={id}>{short(id)}</span>;

/** The proof’s own answers, as the failed frame’s “What was tried” box names them. */
const proofLine = (check: EscapeCheckRow): ReactNode =>
  check.proof ? (
    <>Fails on the escape: {check.proof.fails_on_escape ? "yes" : "no"} · Passes on the fix: {check.proof.passes_on_fix ? "yes" : "no"}</>
  ) : (
    "Not run yet"
  );

/** The check’s own words for why a failed round did not prove itself: the design’s sentence for the case it drew, the
 * proof’s or the core’s own sentence for the states it did not. */
function failedWhy(check: EscapeCheckRow): string {
  const rule = "A check is added only when it fails on the escaped merge and passes on the fix.";
  const proof = check.proof;
  if (!proof) return `${rule} ${check.note ?? check.line} Nothing was merged.`;
  if (!proof.fails_on_escape && proof.passes_on_fix)
    return `${rule} This one passes on the escaped merge too, so it is marked failed: adding it would give false comfort. Nothing was merged.`;
  if (proof.fails_on_escape && !proof.passes_on_fix) return `${rule} This one fails on the fix too, so it proves nothing. Nothing was merged.`;
  return `${rule} This one passes on the escaped merge and fails on the fix, the wrong way round. Nothing was merged.`;
}

/** What the retry dialog names as the round’s last result, from the proof the read carries. */
function lastResult(check: EscapeCheckRow): string {
  const proof = check.proof;
  if (!proof) return check.note ?? "Not proven";
  if (!proof.fails_on_escape && proof.passes_on_fix) return "Passed on the escaped merge too";
  if (proof.fails_on_escape && !proof.passes_on_fix) return "Failed on the fix too";
  return "Passed on the escape and failed on the fix, the wrong way round";
}

/** The header chip of a check’s state, as each frame draws it. */
export function CheckChip({ check }: { check: EscapeCheckRow }) {
  if (check.state === "writing" && check.awaiting_approval)
    return (
      <span className="cx-st cx-warn">
        <i className="cx-sd" />
        Needs your approval
      </span>
    );
  if (check.state === "writing")
    return (
      <span className="cx-st cx-info">
        <i className="cx-spin" aria-label="Working" role="img" />
        Check being written
      </span>
    );
  if (check.state === "failed")
    return (
      <span className="cx-st cx-warn">
        <i className="cx-sd" />
        Check failed
      </span>
    );
  const ok = check.state === "proven" || check.state === "merged" ? " cx-ok" : "";
  return <span className={`cx-apchip${ok}`}>{checkStateLabel(check.state)}</span>;
}

/** Retry asks first: the dialog the `escape-check-retry` frame draws over the failed check. */
function RetryDialog({ check, busy, onClose, onRetry }: { check: EscapeCheckRow; busy: boolean; onClose(): void; onRetry(): void }) {
  return (
    <div className="cx-cwov">
      <div>
        <div className="cx-cwdlg" style={{ width: "600px" }} role="dialog" aria-modal="true" aria-label="Retry this check?">
          <h4>Retry this check?</h4>
          <p>It goes back to Proposed for a new round. Retry starts nothing: no worker runs until you accept again. The failed round stays in the trail.</p>
          <div className="cx-fbx">
            <div className="cx-fkv">
              <span>New test file</span>
              <b>{check.path}</b>
              <span>Attempts so far</span>
              <b>{check.attempts}</b>
              <span>Last result</span>
              <b>{lastResult(check)}</b>
            </div>
          </div>
          <div className="cx-ra">
            <button type="button" className="cx-ghost" onClick={onClose}>Cancel</button>
            <button type="button" className="cx-btn" disabled={busy} onClick={onRetry}>Retry</button>
          </div>
        </div>
      </div>
    </div>
  );
}

/** Withdraw asks first, and says what it stops and when Crew refuses, as the `escape-check-withdraw` frame draws. */
function WithdrawDialog({ check, busy, onClose, onWithdraw }: { check: EscapeCheckRow; busy: boolean; onClose(): void; onWithdraw(): void }) {
  return (
    <div className="cx-cwov">
      <div>
        <div className="cx-cwdlg" style={{ width: "600px" }} role="dialog" aria-modal="true" aria-label="Withdraw this check?">
          <h4>Withdraw this check?</h4>
          <p>This stops it: a worker still writing is cancelled, a proof still running is stopped and a queued merge of it is turned down. A withdrawn check can be proposed again.</p>
          <div className="cx-fbx">
            <div className="cx-fkv">
              <span>Now</span>
              <b>{check.state.charAt(0).toUpperCase()}{check.state.slice(1)}</b>
              <span>If the merge is landing</span>
              <b>Crew refuses: “The merge is landing right now. Withdraw the check after it ends.”</b>
              <span>If it already merged</span>
              <b>The answer says: “Already merged: the merge could not be stopped”</b>
            </div>
          </div>
          <div className="cx-ra">
            <button type="button" className="cx-ghost" onClick={onClose}>Cancel</button>
            <button type="button" className="cx-btn" disabled={busy} onClick={onWithdraw}>Withdraw the check</button>
          </div>
        </div>
      </div>
    </div>
  );
}

/** `escape-check-approve`: where the machine cannot confine the proof, the person approves the exact change first. */
function ApproveBox({ check, busy, onApprove, onWithdraw }: { check: EscapeCheckRow; busy: boolean; onApprove(): void; onWithdraw(): void }) {
  return (
    <div className="cx-fbx cx-amber">
      <h6>Approve the proof</h6>
      <p>This computer can’t confine the proof, so it would run with your own access. It runs only if you approve exactly this change. This is shown only where the proof can’t be confined.</p>
      <div className="cx-fkv">
        <span>Adds</span>
        <b>
          {check.diff_paths.length > 0
            ? `${check.diff_paths.length} file${check.diff_paths.length === 1 ? "" : "s"}: ${check.diff_paths.join(", ")}`
            : "No file was read"}
        </b>
        <span>Commit</span>
        <b>
          <span style={mono}>{check.diff_commit}</span>
        </b>
        <span>Hash (SHA-256)</span>
        <b>
          <span style={mono}>{check.diff_hash}</span>
        </b>
        <span>Then it runs</span>
        <b>
          <span style={mono}>{check.run}</span>
        </b>
        <span>On</span>
        <b>the escaped merge (must fail) and {short(check.fix_commit)} (must pass), each in a fresh scratch folder</b>
        <span>With</span>
        <b>Your own access: no sandbox, no network limit</b>
      </div>
      <div className="cx-t13row" style={{ marginTop: "8px" }}>
        <button type="button" className="cx-btn" disabled={busy} onClick={onApprove}>Approve this change</button>
        <button type="button" className="cx-ghost" disabled={busy} onClick={onWithdraw}>Withdraw</button>
      </div>
      <p style={{ marginTop: "8px" }}>The approval names this commit and this hash. If the worker changes the file again, the approval is dropped and you approve the new one.</p>
    </div>
  );
}

/** `escape-check-proposed`: a proposal is only a row. Nothing has run and no task exists. */
function ProposedCard({ check, busy, onAccept, onWithdraw }: { check: EscapeCheckRow; busy: boolean; onAccept(): void; onWithdraw(): void }) {
  return (
    <div className="cx-fbx">
      <h6>The check</h6>
      <div className="cx-fkv">
        <span>State</span>
        <b>
          <span className="cx-apchip">Proposed</span>
        </b>
        <span>New test file</span>
        <b>{check.path}</b>
        <span>It asserts</span>
        <b>{check.asserts}</b>
        <span>How it runs</span>
        <b>
          <span style={mono}>{check.run}</span>
        </b>
        <span>Must pass on</span>
        <b>
          <Commit id={check.fix_commit} />
        </b>
        <span>Proof before it is added</span>
        <b>It must fail on the escaped merge and pass on {short(check.fix_commit)}. If it passes on both, the check is marked failed and you decide</b>
      </div>
      <div className="cx-t13row" style={{ marginTop: "8px" }}>
        <button type="button" className="cx-btn" disabled={busy} onClick={onAccept}>Accept: start a worker</button>
        <button type="button" className="cx-ghost" disabled={busy} onClick={onWithdraw}>Withdraw</button>
      </div>
      <p style={{ marginTop: "8px" }}>
        Only a confirmed escape with a test gap can have a check. A tooling gap has no proposal <Planned why={WHY.tooling} />.
      </p>
    </div>
  );
}

/** `escape-check-writing`: a crew task writes the one test file. The page shows the task and the one act left. */
function WritingCard({ check, awaiting, busy, onWithdraw }: { check: EscapeCheckRow; awaiting: boolean; busy: boolean; onWithdraw(): void }) {
  return (
    <div className="cx-fbx">
      <h6>The check</h6>
      <div className="cx-fkv">
        <span>State</span>
        <b>
          {awaiting ? (
            <span className="cx-st cx-warn">
              <i className="cx-sd" />
              Writing · waiting for your approval
            </span>
          ) : (
            <span className="cx-st cx-info">
              <i className="cx-spin" aria-label="Working" role="img" />
              Writing
            </span>
          )}
        </b>
        <span>New test file</span>
        <b>{check.path}</b>
        <span>It asserts</span>
        <b>{check.asserts}</b>
        <span>How it runs</span>
        <b>
          <span style={mono}>{check.run}</span>
        </b>
        <span>Must pass on</span>
        <b>
          <Commit id={check.fix_commit} />
        </b>
        <span>Worker</span>
        <b>Task {check.task_id} · writing the test file</b>
        <span>Proof</span>
        <b>Runs when the worker’s change is read: once on the escaped merge, once on {short(check.fix_commit)}</b>
        <span>Where it runs</span>
        <b>The proof runs in a sandbox where the machine can restrict it: no network, writes only to its own scratch folder. Where it cannot, it waits for your approval</b>
        <span>Attempts</span>
        <b>{check.attempts}</b>
      </div>
      {awaiting ? null : (
        <div className="cx-t13row" style={{ marginTop: "8px" }}>
          <button type="button" className="cx-ghost" disabled={busy} onClick={onWithdraw}>Withdraw</button>
        </div>
      )}
    </div>
  );
}

/** `escape-check-proven`: the proof held, and Crew queued the check through the merge queue like any change. */
function ProvenCard({ check, busy, onWithdraw }: { check: EscapeCheckRow; busy: boolean; onWithdraw(): void }) {
  return (
    <div className="cx-fbx">
      <h6>The check</h6>
      <div className="cx-fkv">
        <span>State</span>
        <b>
          <span className="cx-apchip cx-ok">Proven</span>
        </b>
        <span>New test file</span>
        <b>{check.path}</b>
        <span>It asserts</span>
        <b>{check.asserts}</b>
        <span>How it runs</span>
        <b>
          <span style={mono}>{check.run}</span>
        </b>
        <span>Must pass on</span>
        <b>
          <Commit id={check.fix_commit} />
        </b>
        <span>Proof</span>
        <b>
          {check.proof
            ? `Fails on ${short(check.proof.escaped_commit)}: ${check.proof.fails_on_escape ? "yes" : "no"} · Passes on ${short(check.proof.fix_commit)}: ${check.proof.passes_on_fix ? "yes" : "no"}`
            : "Not run yet"}
        </b>
        <span>Merge</span>
        <b>Queued like any change, with its own risk tier, holds and required checks</b>
        <span>Attempts</span>
        <b>{check.attempts}</b>
      </div>
      <div className="cx-t13row" style={{ marginTop: "8px" }}>
        <Planned why={WHY.queue} />
        <Btn className="cx-ghost" reason={`Planned: ${WHY.queue}`}>Open the merge queue</Btn>
        <button type="button" className="cx-ghost" disabled={busy} onClick={onWithdraw}>Withdraw</button>
      </div>
      <p style={{ marginTop: "8px" }}>
        A proof is evidence, not a verdict: one run on each commit can’t tell a test that fails because of the bug from one that fails by luck. Read the test before it merges.
      </p>
    </div>
  );
}

/** `escape-check-invalid`: the check did not prove itself, and the person decides what happens next. */
function FailedCard({ row, check, busy, onRetry, onWithdraw }: { row: EscapeRow; check: EscapeCheckRow; busy: boolean; onRetry(): void; onWithdraw(): void }) {
  return (
    <>
      <div className="cx-fbx cx-amber">
        <h6>The check did not prove itself</h6>
        <p>{failedWhy(check)}</p>
        <div className="cx-t13row" style={{ marginTop: "0" }}>
          <button type="button" className="cx-btn" disabled={busy} onClick={onRetry}>Retry</button>
          <button type="button" className="cx-ghost" disabled={busy} onClick={onWithdraw}>Withdraw</button>
        </div>
        <p style={{ marginTop: "8px" }}>Retry sends it back to Proposed for a new round and starts nothing: you accept again. Withdraw ends it; you can propose again later.</p>
      </div>
      <div className="cx-fbx">
        <h6>What was tried</h6>
        <div className="cx-fkv">
          <span>New test file</span>
          <b>{check.path}</b>
          <span>How it ran</span>
          <b>
            <span style={mono}>{check.run}</span>
          </b>
          {check.proof ? (
            <>
              <span>On {short(check.proof.escaped_commit)} (escaped merge)</span>
              <b>{check.proof.fails_on_escape ? "Failed · as it must" : "Passed · it had to fail"}</b>
              <span>On {short(check.proof.fix_commit)} (the fix)</span>
              <b>{check.proof.passes_on_fix ? "Passed" : "Failed · it had to pass"}</b>
            </>
          ) : null}
          <span>Proof</span>
          <b>{proofLine(check)}</b>
          <span>Attempts</span>
          <b>{check.attempts}</b>
        </div>
      </div>
      <div className="cx-fbx">
        <h6>Still recorded</h6>
        <p>The escape stays confirmed, against {row.engine} · {row.areas.join(" · ")}, while you decide. A failed check changes no count.</p>
      </div>
    </>
  );
}

/** `escape-check-withdrawn`: final for the check, and a new proposal replaces this one. */
function WithdrawnCard({ check }: { check: EscapeCheckRow }) {
  return (
    <div className="cx-fbx">
      <h6>The check</h6>
      <div className="cx-fkv">
        <span>State</span>
        <b>
          <span className="cx-apchip">Withdrawn</span>
        </b>
        <span>New test file</span>
        <b>{check.path}</b>
        <span>It asserts</span>
        <b>{check.asserts}</b>
        <span>How it runs</span>
        <b>
          <span style={mono}>{check.run}</span>
        </b>
        <span>Must pass on</span>
        <b>
          <Commit id={check.fix_commit} />
        </b>
      </div>
      <div className="cx-t13row" style={{ marginTop: "8px" }}>
        <Planned why={WHY.check} />
        <Btn className="cx-btn" reason={`Planned: ${WHY.check}`}>Propose a check</Btn>
      </div>
      <p style={{ marginTop: "8px" }}>The escape is still confirmed. A new proposal replaces this one.</p>
    </div>
  );
}

/** One act on a check, sent only by the person’s own click. The dialogs confirm first; the page owns the write and the read. */
export type CheckAct = "accept" | "approve" | "retry" | "withdraw";

/**
 * The check of one escape, read as it stands, as the check frames of Crew Designs v79 draw it: the rows and acts the read
 * and the routes carry, and a Planned pill where a frame draws what the app does not have. `merged` keeps the detail
 * page’s own merged card (no frame of this slice draws it).
 */
export function CheckCard({ row, check, busy, onAct }: { row: EscapeRow; check: EscapeCheckRow; busy: boolean; onAct(kind: CheckAct): void }) {
  const [ask, setAsk] = useState<"retry" | "withdraw" | null>(null);
  const confirming = (kind: CheckAct) => () => {
    setAsk(null);
    onAct(kind);
  };
  let card: ReactNode = null;
  if (check.state === "proposed") card = <ProposedCard check={check} busy={busy} onAccept={() => onAct("accept")} onWithdraw={() => setAsk("withdraw")} />;
  else if (check.state === "writing")
    card = (
      <>
        {check.awaiting_approval && check.diff_commit && check.diff_hash ? (
          <ApproveBox check={check} busy={busy} onApprove={() => onAct("approve")} onWithdraw={() => setAsk("withdraw")} />
        ) : null}
        <WritingCard check={check} awaiting={check.awaiting_approval} busy={busy} onWithdraw={() => setAsk("withdraw")} />
      </>
    );
  else if (check.state === "proven") card = <ProvenCard check={check} busy={busy} onWithdraw={() => setAsk("withdraw")} />;
  else if (check.state === "failed") card = <FailedCard row={row} check={check} busy={busy} onRetry={() => setAsk("retry")} onWithdraw={() => setAsk("withdraw")} />;
  else if (check.state === "withdrawn") card = <WithdrawnCard check={check} />;
  if (!card) return null;
  return (
    <>
      {card}
      {ask === "retry" ? <RetryDialog check={check} busy={busy} onClose={() => setAsk(null)} onRetry={confirming("retry")} /> : null}
      {ask === "withdraw" ? <WithdrawDialog check={check} busy={busy} onClose={() => setAsk(null)} onWithdraw={confirming("withdraw")} /> : null}
    </>
  );
}
