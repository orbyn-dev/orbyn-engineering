import { useEffect, useRef, type ReactNode } from "react";
import { Button } from "../../../../ui/controls";
import { chainOf, verdictOf, type Step } from "./chain";
import { GUARD_WORKERS, MODE_COPY } from "./copy";

import type { ApprovalRow, Mode } from "./model";
import { Chip, engineLine, Facts, ResultChip, RISK_LABEL } from "./parts";

const MODE_NAME = { off: MODE_COPY.off.name, on: MODE_COPY.on.name, full: MODE_COPY.full.name };

export interface DrawerActions {
  /** The host's own destinations. A missing one hides its button: nothing is drawn that cannot open. */
  openLedger?(row: ApprovalRow): void;
  openDisagreement?(row: ApprovalRow): void;
  openRequest?(row: ApprovalRow): void;
  seeHardDenied?(): void;
  signInSecondEngine?(): void;
  approveMyself(row: ApprovalRow): void;
  approveOnce(row: ApprovalRow): void;
  deny(row: ApprovalRow): void;
  dismiss(row: ApprovalRow): void;
  close(): void;
}

const isGuarded = (row: ApprovalRow) => row.hard === "guard" || row.hard === "guard_two_keys";

function StepCard({ step }: { step: Step }) {
  return (
    <div className={`ap-step ap-step-${step.tone}`}>
      <h4>{step.title}</h4>
      <p>{step.text}</p>
    </div>
  );
}

function Chain({ steps }: { steps: Step[] }) {
  return (
    <ol className="ap-chain" aria-label="Decision chain">
      {steps.map((step, at) => (
        <li key={`${step.title}-${at}`}>
          <StepCard step={step} />
        </li>
      ))}
    </ol>
  );
}

function Side({ keys }: { keys: { head: string; who: string; text: string }[] }) {
  return (
    <div className="ap-keys">
      {keys.map((key) => (
        <div key={key.head} className="ap-step ap-step-ok">
          <h4>{key.head}</h4>
          <small>{key.who}</small>
          <p>{key.text}</p>
        </div>
      ))}
    </div>
  );
}

function sideBySide(row: ApprovalRow): { head: string; who: string; text: string }[] {
  const lead = row.leadBy;
  const leadReason = row.lead.state === "yes" ? row.lead.reason : "";
  const second = row.second;
  if (row.result === "approved_you") {
    return [
      { head: "The lead", who: lead ? `${lead.engine} · ${lead.model} · ${lead.vendor}` : "", text: `Yes: ${leadReason}` },
      { head: "You", who: "Approved once, on this computer", text: "You read the command and the target, and approved this one command." },
    ];
  }
  return [
    { head: "Key 1 · the lead", who: lead ? `${lead.engine} · ${lead.model} · ${lead.vendor}` : "", text: `Yes: ${leadReason}` },
    second.state === "yes"
      ? { head: "Key 2 · a second vendor", who: `${engineLine(second.by)} · ${second.by.vendor}`, text: `Yes: ${second.reason}` }
      : { head: "Key 2 · a second vendor", who: "", text: "" },
  ];
}

function Button2({ children, onClick, tone = "secondary" }: { children: ReactNode; onClick(): void; tone?: "primary" | "secondary" | "quiet" }) {
  return <Button small tone={tone} onClick={onClick}>{children}</Button>;
}

function Footer({ row, act }: { row: ApprovalRow; act: DrawerActions }) {
  const close = <Button2 key="close" onClick={act.close} tone="quiet">Close</Button2>;
  const ledger = act.openLedger ? <Button2 key="ledger" onClick={() => act.openLedger?.(row)}>Open in the ledger</Button2> : null;
  const self = <Button2 key="self" onClick={() => act.approveMyself(row)}>Approve it myself</Button2>;
  const signIn = act.signInSecondEngine ? <Button2 key="signin" onClick={() => act.signInSecondEngine?.()}>Sign in to a second engine</Button2> : null;
  const list: (ReactNode | null)[] = (() => {
    switch (row.result) {
      case "denied_rule":
        return [act.seeHardDenied ? <Button2 key="hard" onClick={() => act.seeHardDenied?.()}>See the hard-denied list</Button2> : null, close];
      case "keys_disagree":
        return [act.openDisagreement ? <Button2 key="dis" onClick={() => act.openDisagreement?.(row)}>Open the disagreement</Button2> : null, close];
      case "person_only":
        return [act.openRequest ? <Button2 key="req" onClick={() => act.openRequest?.(row)}>Open the request</Button2> : null, <Button2 key="dismiss" onClick={() => act.dismiss(row)} tone="quiet">Dismiss</Button2>];
      case "no_second_key":
        return isGuarded(row) ? [signIn, self, close] : [signIn, <Button2 key="dismiss" onClick={() => act.dismiss(row)} tone="quiet">Dismiss</Button2>];
      case "denied_second_key":
        return [self, ledger, close];
      case "held_you":
        return [
          <Button key="once" small tone="primary" onClick={() => act.approveOnce(row)}>Approve once</Button>,
          <Button key="deny" small onClick={() => act.deny(row)}>Deny</Button>,
        ];
      default:
        return [ledger, close];
    }
  })();
  return <footer className="ap-drawer-foot">{list}</footer>;
}

/** One request's whole chain, opened from a row. Escape closes it and focus goes back to the row. */
export function DecisionDrawer({ row, act, mode }: { row: ApprovalRow; act: DrawerActions; mode: Mode }) {
  const ref = useRef<HTMLElement>(null);
  useEffect(() => ref.current?.focus(), [row.id]);
  const guarded = isGuarded(row);
  const held = row.result === "held_you";
  const sides = row.result === "approved_two_keys" || row.result === "approved_you";
  const verdict = verdictOf(row);
  const guard = row.guard;
  const facts = row.facts.length ? row.facts : [];
  return (
    <aside
      ref={ref}
      tabIndex={-1}
      className="ap-drawer"
      aria-label={row.title}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          act.close();
        }
      }}
    >
      <header className="ap-drawer-head">
        <h3>{row.title}</h3>
        <div className="ap-chips">
          <ResultChip result={row.result} />
          {guarded ? <Chip tone="dim">Guard</Chip> : row.risk ? <Chip tone="plain">{RISK_LABEL[row.risk]}</Chip> : row.hard === "denied" ? <Chip tone="plain">Hard limit</Chip> : row.hard === "person_only" ? <Chip tone="plain">Person-only</Chip> : null}
        </div>
      </header>
      <div className="ap-drawer-body">
        <p className="ap-asked">
          Asked by <b>{engineLine(row.asker)}</b>
          {row.branch ? ` · ${row.branch}` : ""}
        </p>
        {guard ? (
          <Facts rows={[
            { label: "Target", text: `${guard.target} · ${guard.kind}${held ? " · guarded" : guard.markedBy ? `, marked by the rule ${guard.markedBy}` : ""}` },
            ...(held ? [{ label: "Autopilot", text: `${MODE_NAME[mode]}: the guard holds this command for you` }] : []),
            ...(guard.at ? [{ label: held ? "Waiting since" : row.result === "approved_you" ? "Approved at" : row.result === "approved_two_keys" ? "Ran at" : "Decided at", text: guard.exit !== null ? `${guard.at} · exit ${guard.exit}` : guard.at }] : []),
          ]} />
        ) : facts.length ? <Facts rows={facts} /> : null}
        {row.block ? <pre className="ap-cmd"><code>{row.block}</code></pre> : null}
        {held ? (
          <>
            <section className="ap-box">
              <h4>Why it is held</h4>
              <p>This command deletes data in a production-linked target. Under On and Off a guarded command waits for you. The exact command, the target and the reason are here, and nothing runs until you approve it.</p>
            </section>
            <p className="ap-muted">A held command is never approved in a batch: each one is approved or denied on its own, and your answer is logged with the time.</p>
          </>
        ) : sides ? (
          <>
            <Side keys={sideBySide(row)} />
            <p className="ap-muted">
              {row.result === "approved_you"
                ? "You are the final authority; this isn’t counted as a second key. The ledger keeps your approval with the command and its exit status."
                : "Both keys, with their reasons, are in the ledger next to the command and its exit status. The guard applies the same way in every mode."}
            </p>
          </>
        ) : (
          <>
            <h4 className="ap-sub">Decision chain</h4>
            <Chain steps={chainOf(row)} />
            {verdict.steps.map((step) => <StepCard key={step.title} step={step} />)}
            {verdict.foot ? <p className="ap-muted">{verdict.foot}</p> : null}
          </>
        )}
        {guarded ? <p className="ap-muted">{GUARD_WORKERS}</p> : null}
      </div>
      <Footer row={row} act={act} />
    </aside>
  );
}
