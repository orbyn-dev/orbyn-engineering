import { Dialog } from "../../../../ui/Dialog";
import { HARD_DENIED, HARD_FOOT } from "./copy";
import type { AutopilotRule } from "../../../../generated/AutopilotRule";
import { useAutopilotGuards } from "./useCrew";
import { Loading } from "../../../../product/ui/states";

/** The always-denied list from the service, in the designs' order and words; a rule the designs do not name keeps the core's own words. */
export function listed(rules: AutopilotRule[]): [string, string][] {
  const order = HARD_DENIED.map(([id]) => id);
  const byId = new Map(rules.map((rule) => [rule.id, rule.rule]));
  const known = HARD_DENIED.filter(([id]) => byId.has(id));
  const extra = rules.filter((rule) => !order.includes(rule.id)).map((rule): [string, string] => [rule.id, rule.rule.charAt(0).toUpperCase() + rule.rule.slice(1)]);
  return [...known, ...extra];
}

export function HardDeniedList({ rules, heading = "Always denied, in every mode" }: { rules: AutopilotRule[]; heading?: string }) {
  return (
    <section className="ap-box" aria-label={heading}>
      <h4>{heading}</h4>
      <ul className="ap-list">
        {listed(rules).map(([id, text]) => <li key={id}>{text}</li>)}
      </ul>
      <p className="ap-muted">{HARD_FOOT}</p>
    </section>
  );
}

/** Opened from the setting: what is denied in every mode, what is always allowed, and why it is code. */
export function HardDeniedPage({ rules }: { rules: AutopilotRule[] }) {
  return (
    <div className="ap-stack">
      <HardDeniedList rules={rules} />
      <div className="ap-two">
        <section className="ap-box">
          <h4>Always allowed</h4>
          <p>Reads and writes inside a worker’s leased paths. Running the brief’s checks.</p>
        </section>
        <section className="ap-box">
          <h4>Allowed with a second key</h4>
          <p>Network, installing dependencies, running generated scripts, touching CI files: the lead and an approver on a different vendor must both say yes.</p>
        </section>
      </div>
      <section className="ap-box">
        <h4>Why code and not a model</h4>
        <p>A model that approves requests can be fooled by text it reads. The limits are enforced in code, and the lead sees a structured summary of each action, never raw page or tool text.</p>
      </section>
    </div>
  );
}

export function HardDeniedDialog({ onClose }: { onClose(): void }) {
  const guards = useAutopilotGuards();
  return (
    <Dialog title="What Autopilot can never do" onClose={onClose} className="ap-dialog">
      {guards.status === "ready" ? <HardDeniedPage rules={guards.data.always_denied} /> : <Loading label="Reading the list" rows={3} />}
    </Dialog>
  );
}
