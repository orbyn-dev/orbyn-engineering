import { useState } from "react";
import type { EscapeRow } from "../../../../generated/EscapeRow";
import { Button, Field } from "../../../../ui/controls";
import { Dialog } from "../../../../ui/Dialog";
import { againstOf, gapLabel, nameOf } from "./model";

/** What the person typed, as the service takes it: `fix_commit` is left out when the person left it empty. */
export type Proposal = { path: string; asserts: string; run: string; fix_commit?: string };

/** The service's own limits on what and how (`EscapeCheckPropose`). */
const LIMIT = 300;

/**
 * `escape-create-check` of Crew Designs v20 on the real propose route: the person gives the one new test file, what it
 * asserts, the command that runs it and, when the escape recorded no revert or fix, the commit it must pass on. Nothing is
 * sent before "Propose the check", and a proposal only writes a row: no task starts until the person accepts it on the page.
 * `onPropose` answers null when the service took it, or the service's own words when it refused; the dialog then stays open
 * with what was typed. The path rules are the service's (a test file in a tests folder, never a policy file), so the form
 * checks none of them and shows the refusal as it comes.
 */
export function ProposeDialog({ row, busy, onClose, onPropose }: { row: EscapeRow; busy: boolean; onClose(): void; onPropose(proposal: Proposal): Promise<string | null> }) {
  const [path, setPath] = useState("");
  const [asserts, setAsserts] = useState("");
  const [run, setRun] = useState("");
  const [fix, setFix] = useState("");
  const [refusal, setRefusal] = useState<string | null>(null);
  const ready = path.trim() !== "" && asserts.trim() !== "" && run.trim() !== "";
  const send = () => {
    if (!ready || busy) return;
    setRefusal(null);
    void onPropose({ path: path.trim(), asserts: asserts.trim(), run: run.trim(), ...(fix.trim() ? { fix_commit: fix.trim() } : {}) }).then((refused) => {
      if (refused === null) onClose();
      else setRefusal(refused);
    });
  };
  return (
    <Dialog
      title="Propose a check from this escape"
      busy={busy}
      onClose={onClose}
      footer={
        <>
          <Button small disabled={busy} onClick={onClose}>Cancel</Button>
          <Button small tone="primary" disabled={busy || !ready} onClick={send}>Propose the check</Button>
        </>
      }
    >
      <p>
        You give one new test file that fails on the escaped merge and passes on the fix, so the same change can’t reach main again.
        Nothing runs until you accept the proposal.
      </p>
      <div className="cx-fbx">
        <div className="cx-fkv">
          <span>What escaped</span>
          <b>{nameOf(row)}</b>
          <span>Merge</span>
          <b>{row.merge_task_id}</b>
          <span>Counts against</span>
          <b>{againstOf(row)}</b>
          <span>Gap</span>
          <b>{row.gap ? gapLabel[row.gap] : "Not set yet"}</b>
        </div>
      </div>
      <Field label="New test file" hint="One new file in a tests folder, never a config, manifest or policy file and never .orbyn/crew.json. Crew refuses anything else.">
        {(props) => <input {...props} data-dialog-focus type="text" value={path} maxLength={200} disabled={busy} placeholder="tests/auth/return-to.test.ts" onChange={(event) => setPath(event.target.value)} />}
      </Field>
      <Field label="What it asserts" hint={`In plain words, at most ${LIMIT} characters.`}>
        {(props) => <input {...props} type="text" value={asserts} maxLength={LIMIT} disabled={busy} onChange={(event) => setAsserts(event.target.value)} />}
      </Field>
      <Field label="How it runs" hint="The repository has no test selector Crew can trust, so give the command that runs this one file.">
        {(props) => <input {...props} type="text" value={run} maxLength={LIMIT} disabled={busy} placeholder="npm test -- tests/auth/return-to.test.ts" onChange={(event) => setRun(event.target.value)} />}
      </Field>
      <Field label="Commit it must pass on (optional)" hint="Leave it empty to use the revert or fix Crew recorded. An escape with none needs one.">
        {(props) => <input {...props} type="text" value={fix} disabled={busy} onChange={(event) => setFix(event.target.value)} />}
      </Field>
      <p>
        The proof rule: it must fail on the escaped merge and pass on the fix. If it passes on both, the check is marked failed and you decide.
        Proposing writes no task; accepting starts the worker, and the check then merges through the merge queue like any change.
      </p>
      {refusal ? <p className="field-error" role="alert">{refusal}</p> : null}
    </Dialog>
  );
}

/** The "Propose a check" button and the dialog it opens: the one place a proposal is started, always from the person's own click. */
export function ProposeAction({ row, busy, onPropose }: { row: EscapeRow; busy: boolean; onPropose(proposal: Proposal): Promise<string | null> }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" className="cx-btn" disabled={busy} onClick={() => setOpen(true)}>Propose a check</button>
      {open ? <ProposeDialog row={row} busy={busy} onClose={() => setOpen(false)} onPropose={onPropose} /> : null}
    </>
  );
}
