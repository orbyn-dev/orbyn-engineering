import { useEffect, useRef, type MouseEvent, type ReactNode } from "react";
import { Button } from "../../../../ui/controls";
import { chainOf, verdictOf, type Step } from "./chain";
import { NOT_CONNECTED } from "./api";
import { GUARD_WORKERS, HELD, MODE_COPY } from "./copy";

import type { ApprovalRow, Mode } from "./model";
import { HeldBody, HeldFooter, Said } from "./HeldCard";
import { phaseOf, type HoldControl } from "./held";
import { Chip, engineLine, Facts, ResultChip, RISK_LABEL } from "./parts";

const MODE_NAME = { off: MODE_COPY.off.name, on: MODE_COPY.on.name, full: MODE_COPY.full.name };

export interface DrawerActions {
  /** The host's own destinations. A missing one hides its button: nothing is drawn that cannot open. */
  openLedger?(row: ApprovalRow): void;
  openDisagreement?(row: ApprovalRow): void;
  openRequest?(row: ApprovalRow): void;
  seeHardDenied?(): void;
  signInSecondEngine?(): void;
  /** "Approve it myself": the same flow as Approve once, handed the click. Only the desktop app offers it. */
  approveMyself(row: ApprovalRow, event: MouseEvent<HTMLButtonElement>): void;
  /** This window is the desktop app, so approving once can ask the shell. A browser tab says it needs the app. */
  desktop?: boolean;
  /** The shell's dialog is open. */
  asking?: boolean;
  /** The row whose last approval was not confirmed as the person's. */
  refused?: string | null;
  close(): void;
}

const isGuarded = (row: ApprovalRow) => row.hard === "guard" || row.hard === "guard_two_keys";

/** The Crew Designs frame a guarded command's card is in, for `data-frame`. */
export function guardFrame(row: ApprovalRow, asking: boolean): string | undefined {
  switch (row.result) {
    case "held_you":
      return "guard-held";
    case "no_second_key":
      return asking ? "guard-approve-myself" : "guard-denied-no-second";
    case "denied_second_key":
      return "guard-denied";
    case "approved_two_keys":
      return "guard-approved";
    case "approved_you":
      return row.guard?.exitUnknown ? "guard-approved-person-unknown" : "guard-approved-person";
    default:
      return undefined;
  }
}

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

/** The engine as the card names it: the stored row has the engine id, and the model and vendor only when something knows them. */
const whoLine = (engine: { engine: string; model: string; vendor: string }) => [engine.engine, engine.model, engine.vendor].filter(Boolean).join(" · ");

function sideBySide(row: ApprovalRow): { head: string; who: string; text: string }[] {
  const lead = row.leadBy;
  const second = row.second;
  // The ledger row of an approval by the person holds no answer of the lead's, so that card is drawn only when there is one.
  const leadKey = (head: string) => (row.lead.state === "yes" ? [{ head, who: lead ? whoLine(lead) : "", text: `Yes: ${row.lead.reason}` }] : []);
  if (row.result === "approved_you") {
    return [
      ...leadKey("The lead"),
      { head: "You", who: "Approved once, on this computer", text: "You read the command and the target, and approved this one command." },
    ];
  }
  return [
    ...leadKey("Key 1 · the lead"),
    second.state === "yes"
      ? { head: "Key 2 · a second vendor", who: whoLine(second.by), text: `Yes: ${second.reason}` }
      : { head: "Key 2 · a second vendor", who: "", text: "" },
  ];
}

/** A button for an answer the service cannot take yet: disabled, with the reason in its description. */
function Waiting({ children, tone = "secondary" }: { children: ReactNode; tone?: "primary" | "secondary" | "quiet" }) {
  return <Button small tone={tone} disabled title={NOT_CONNECTED} aria-description={NOT_CONNECTED}>{children}</Button>;
}

function Button2({ children, onClick, tone = "secondary", disabled }: { children: ReactNode; onClick(event: MouseEvent<HTMLButtonElement>): void; tone?: "primary" | "secondary" | "quiet"; disabled?: boolean }) {
  return <Button small tone={tone} disabled={disabled} onClick={onClick}>{children}</Button>;
}

function Footer({ row, act }: { row: ApprovalRow; act: DrawerActions }) {
  const close = <Button2 key="close" onClick={act.close} tone="quiet">Close</Button2>;
  const ledger = act.openLedger ? <Button2 key="ledger" onClick={() => act.openLedger?.(row)}>Open in the ledger</Button2> : null;
  const self = act.desktop && row.held ? <Button2 key="self" disabled={act.asking} onClick={(event) => act.approveMyself(row, event)}>Approve it myself</Button2> : null;
  const signIn = act.signInSecondEngine ? <Button2 key="signin" onClick={() => act.signInSecondEngine?.()}>Sign in to a second engine</Button2> : null;
  let needsApp = false;
  const list: (ReactNode | null)[] = (() => {
    switch (row.result) {
      case "denied_rule":
        return [act.seeHardDenied ? <Button2 key="hard" onClick={() => act.seeHardDenied?.()}>See the hard-denied list</Button2> : null, close];
      case "keys_disagree":
        return [act.openDisagreement ? <Button2 key="dis" onClick={() => act.openDisagreement?.(row)}>Open the disagreement</Button2> : null, close];
      case "person_only":
        return [act.openRequest ? <Button2 key="req" onClick={() => act.openRequest?.(row)}>Open the request</Button2> : null, <Waiting key="dismiss" tone="quiet">Dismiss</Waiting>];
      case "no_second_key":
        needsApp = isGuarded(row);
        return isGuarded(row) ? [signIn, self, close] : [signIn, <Waiting key="dismiss" tone="quiet">Dismiss</Waiting>];
      case "denied_second_key":
        needsApp = true;
        return [self, ledger, close];
      case "held_you":
        return [
          <Waiting key="once" tone="primary">Approve once</Waiting>,
          <Waiting key="deny">Deny</Waiting>,
        ];
      default:
        return [ledger, close];
    }
  })();
  return <footer className="ap-drawer-foot">
      {list}
      {needsApp && !act.desktop ? <small className="ap-muted ap-reason">{HELD.desktopOnly}</small> : null}
      {list.some((item) => item && (item as { type?: unknown }).type === Waiting) ? <small className="ap-muted ap-reason">{NOT_CONNECTED}</small> : null}
    </footer>;
}

/** The head chip of a held command: "Already decided" once it was answered, whoever answered. */
function HeldChip({ held, hold, result }: { held: NonNullable<ApprovalRow["held"]>; hold: HoldControl; result: ApprovalRow["result"] }) {
  const phase = phaseOf(held, hold.closed ?? undefined);
  return phase === "decided" ? <Chip tone="dim">Already decided</Chip> : <ResultChip result={result} />;
}

/** One request's whole chain, opened from a row. Escape closes it and focus goes back to the row. */
export function DecisionDrawer({ row, act, mode, hold }: { row: ApprovalRow; act: DrawerActions; mode: Mode; hold?: HoldControl }) {
  const answers = hold && row.held ? { hold, held: row.held } : null;
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
      data-frame={guarded ? guardFrame(row, act.asking === true) : undefined}
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
          {answers ? <HeldChip held={answers.held} hold={answers.hold} result={row.result} /> : <ResultChip result={row.result} />}
          {guarded ? <Chip tone="dim">Guard</Chip> : row.risk ? <Chip tone="plain">{RISK_LABEL[row.risk]}</Chip> : row.hard === "denied" ? <Chip tone="plain">Hard limit</Chip> : row.hard === "person_only" ? <Chip tone="plain">Person-only</Chip> : null}
          {guard?.exitUnknown ? <Chip tone="dim">Exit status unknown</Chip> : null}
        </div>
      </header>
      {answers ? (
        <>
          <div className="ap-drawer-body"><HeldBody row={row} held={answers.held} hold={answers.hold} mode={mode} /></div>
          <HeldFooter held={answers.held} hold={answers.hold} close={act.close} />
        </>
      ) : (<>
      <div className="ap-drawer-body">
        <p className="ap-asked">
          {row.asker ? <>Asked by <b>{engineLine(row.asker)}</b>{row.branch ? ` · ${row.branch}` : ""}</> : <>Task <b>{row.taskId}</b></>}
        </p>
        {guard ? (
          <Facts rows={[
            { label: "Target", text: `${guard.target} · ${guard.kind}${held ? " · guarded" : guard.markedBy ? `, listed in this device’s ${guard.markedBy}` : ""}` },
            ...(held ? [{ label: "Autopilot", text: `${MODE_NAME[mode]}: the guard holds this command for you` }] : []),
            ...(guard.at ? [{ label: held ? "Waiting since" : row.result === "approved_you" ? "Approved at" : row.result === "approved_two_keys" ? "Ran at" : "Decided at", text: guard.exit !== null ? `${guard.at} · exit ${guard.exit}` : guard.exitUnknown ? `${guard.at} · exit status unknown` : guard.at }] : []),
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
                ? guard?.exitUnknown
                  ? "You are the final authority; this isn’t counted as a second key. The ledger keeps your approval with the command. The engine never reported an exit status for it, so the ledger says “Exit status unknown” instead of a number; that isn’t a failure and isn’t a success."
                  : "You are the final authority; this isn’t counted as a second key. The ledger keeps your approval with the command and its exit status."
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
        {act.refused === row.id ? <Said live note={HELD.refusedNoteOther}>{HELD.refused}</Said> : null}
      </div>
      <Footer row={row} act={act} />
      </>)}
    </aside>
  );
}
