import type { MouseEvent, ReactNode } from "react";
import { Button } from "../../../../ui/controls";
import { HELD } from "./copy";
import { useDesktopApp, type Gesture } from "./presence";
import { Facts, Chip, RISK_LABEL } from "./parts";
import type { HeldCommand, YouState } from "./model";

export interface YouActions {
  /** Each action is drawn only when the Flow host can do it. */
  looksGood?(): void;
  comment?(): void;
  revert?(): void;
  openPr?(): void;
  editRule?(): void;
  turnRuleOff?(): void;
  addReviewer?(): void;
  sendBack?(): void;
  /** Opens the item in the Review queue. It merges nothing. */
  review?(): void;
  approveAndMerge?(): void;
  approveDontMerge?(): void;
  switchToFull?(): void;
}

function Buttons({ items }: { items: [label: string, run: ((event: MouseEvent<HTMLButtonElement>) => void) | undefined, primary?: boolean][] }) {
  const shown = items.filter(([, run]) => run);
  if (!shown.length) return null;
  return (
    <div className="ap-acts">
      {shown.map(([label, run, primary]) => <Button key={label} small tone={primary ? "primary" : "secondary"} onClick={run}>{label}</Button>)}
    </div>
  );
}

const Box = ({ title, children, tone }: { title: string; children: ReactNode; tone?: "am" }) => (
  <section className={tone === "am" ? "ap-box ap-box-am" : "ap-box"}>
    <h4>{title}</h4>
    {children}
  </section>
);

/** The panel beside the You node: neutral when nothing waits on you, amber when a merge does. */
export function YouPanel({ state, act = {} }: { state: YouState; act?: YouActions }) {
  switch (state.kind) {
    case "after_merge":
      return (
        <aside className="ap-you" aria-label="You">
          <header><h3>You: review what reached main</h3><small>Review by {state.reviewBy}</small></header>
          <p className="ap-muted">Merged by Crew at {state.mergedAt} · {RISK_LABEL[state.risk]}</p>
          <Facts rows={state.lines} />
          <Box title="Not verified"><p>{state.notVerified}</p></Box>
          <p className="ap-muted">Nothing here waits on you. The merge already happened; your review can still turn it into a fix brief or a revert.</p>
          <Buttons items={[["Looks good", act.looksGood, true], ["Comment", act.comment], ["Revert", act.revert]]} />
        </aside>
      );
    case "rule":
      return (
        <aside className="ap-you" aria-label="You">
          <header><h3>Merged under a standing rule</h3><div className="ap-chips"><Chip tone="ok">Merged</Chip><Chip tone="plain">Rule: {state.rule}</Chip></div></header>
          <p className="ap-muted">Merged by Crew at {state.mergedAt}</p>
          <p>{state.text} With Autopilot On or Off this is how a change merges without waiting for you; with Full, every change that passes merges.</p>
          <Facts rows={[{ label: "Rule", text: state.rule }, { label: "Set by", text: state.setBy }, { label: "Required stages", text: state.stages }]} />
          <Box title="Where this does not apply"><p>Bots and agents never merge in the skills library; no rule can change that, so there it always waits for a person.</p></Box>
          <Buttons items={[["Open the PR", act.openPr], ["Edit the rule", act.editRule], ["Turn the rule off", act.turnRuleOff]]} />
        </aside>
      );
    case "blocked":
      return (
        <aside className="ap-you" aria-label="You">
          <header><h3>Nothing to review yet</h3><Chip tone="dim">Not merged</Chip></header>
          <Box title={`${state.waiting} of ${state.of} stages before the merge are not green yet`}><Facts rows={state.lines} /></Box>
          <p className="ap-muted">The merge queue starts when they are green, and the change reaches your Review queue after it merges. You can still read the diff, add a reviewer or send the task back.</p>
          <Buttons items={[["Add reviewer", act.addReviewer], ["Send back to Build", act.sendBack]]} />
        </aside>
      );
    case "waiting_on_you":
      return (
        <aside className="ap-you ap-you-wait" aria-label="You">
          <header><h3>Waits for you before the merge</h3></header>
          <Box tone="am" title={`${RISK_LABEL[state.risk]}: waiting for you before the merge`}>
            <Chip tone="am">Needs you</Chip>
          </Box>
          <Box title="Autopilot is On for this repo"><p>Medium and Low risk changes merge by themselves. High risk waits for you here, and nothing else is blocked.</p></Box>
          <Box title={`Why ${state.risk[0]?.toUpperCase() ?? ""}${state.risk.slice(1)}`}><Facts rows={[{ label: "Paths", text: state.paths }, { label: "Size", text: state.size }]} /></Box>
          <Box title="Every stage before the merge is green"><Facts rows={state.lines} /></Box>
          <p className="ap-muted">Not verified: {state.notVerified}</p>
          <Buttons items={[["Review", act.review, true], ["Send back to Build", act.sendBack], ["Switch this repo to Full", act.switchToFull]]} />
        </aside>
      );
    case "off":
      return (
        <aside className="ap-you ap-you-wait" aria-label="You">
          <header><h3>Waits for you before the merge</h3></header>
          <Box tone="am" title="Every change waits for you before the merge"><Chip tone="am">Needs you</Chip></Box>
          <Box title="Autopilot is Off for this repo"><p>Crew proves the work, but nothing merges until you approve it, and agents’ requests come to you, as they did before Autopilot.</p></Box>
          <Box title={`Risk · ${state.risk[0]?.toUpperCase() ?? ""}${state.risk.slice(1)}`}><p>{state.riskNote}</p></Box>
          <Box title="Every stage before the merge is green"><Facts rows={state.lines} /></Box>
          <Buttons items={[["Review", act.review, true], ["Approve, don’t merge", act.approveDontMerge], ["Send back to Build", act.sendBack]]} />
        </aside>
      );
  }
}

/**
 * The card under the Flow graph for a command a guard holds: neutral while it waits on a key, amber for you. Approve once
 * is the desktop app's: it passes the click on, and a browser tab says the desktop app is needed instead of offering it.
 */
export function GuardHold({ held, onApprove, onDeny, onOpenLog }: { held: HeldCommand; onApprove?(event: Gesture): void; onDeny?(): void; onOpenLog?(): void }) {
  const desktop = useDesktopApp();
  return (
    <section className={`ap-box${held.forYou ? " ap-box-am" : ""}`} aria-label="A held command">
      <h4>{held.forYou ? "Build is holding one command" : "Build is holding one command for a second key"}</h4>
      {held.forYou ? <Chip tone="am">Held for you</Chip> : null}
      <Facts rows={[
        { label: "Command", text: held.command },
        { label: "Why", text: held.reason },
        ...(held.forYou ? [] : [{ label: "Key 1", text: held.key1 }, { label: "Key 2", text: held.key2 }]),
        { label: "The task", text: "Keeps going on everything else. It reaches Checks when the command has run or been denied." },
      ]} />
      {held.forYou && !desktop ? <p className="ap-muted">{HELD.desktopOnly}</p> : null}
      <Buttons items={held.forYou ? [["Approve once", desktop ? onApprove : undefined, true], ["Deny", onDeny]] : [["Open in the approval log", onOpenLog]]} />
    </section>
  );
}

/** A Needs you item for a guarded command denied for want of a second key. Nothing is blocked. */
export function GuardNeedsYouItem({ onSignIn, onApproveMyself, onDismiss }: { onSignIn?(): void; onApproveMyself?(event: Gesture): void; onDismiss?(): void }) {
  const desktop = useDesktopApp();
  return (
    <article className="ap-box" aria-label="Sign in to a second engine to allow a command that deletes data in a production-linked target">
      <h4>Sign in to a second engine to allow a command that deletes data in a production-linked target</h4>
      <p className="ap-muted">Risk · High · Hard to undo · Claude Code · Sonnet 5.5 · the guard · Denied for now</p>
      <p>The command was denied for now and the worker took another approach. Nothing is blocked; sign in to a second vendor, or approve the command yourself, once.</p>
      {desktop ? null : <p className="ap-muted">{HELD.desktopOnly}</p>}
      <Buttons items={[["Sign in to a second engine", onSignIn, true], ["Approve it myself", desktop ? onApproveMyself : undefined], ["Dismiss", onDismiss]]} />
    </article>
  );
}
