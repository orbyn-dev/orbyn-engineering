import { Dialog } from "../../../../ui/Dialog";
import { HARD_DENIED, HARD_FOOT } from "./copy";
import type { HardDeniedRule } from "./model";
import { useAutopilotSettings } from "./useCrew";
import { Loading } from "../../../../product/ui/states";

/** The actions denied in every mode, by code. A rule that is not enforced yet is left out of the list. */
export function enforcedRules(rules: HardDeniedRule[]) {
  const on = new Set(rules.filter((rule) => rule.enforced).map((rule) => rule.id));
  return HARD_DENIED.filter(([id]) => on.has(id));
}

export function HardDeniedList({ rules, heading = "Always denied, in every mode" }: { rules: HardDeniedRule[]; heading?: string }) {
  return (
    <section className="ap-box" aria-label={heading}>
      <h4>{heading}</h4>
      <ul className="ap-list">
        {enforcedRules(rules).map(([id, text]) => <li key={id}>{text}</li>)}
      </ul>
      <p className="ap-muted">{HARD_FOOT}</p>
    </section>
  );
}

/** Opened from the setting: what is denied in every mode, what is always allowed, and why it is code. */
export function HardDeniedPage({ rules }: { rules: HardDeniedRule[] }) {
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
  const settings = useAutopilotSettings();
  return (
    <Dialog title="What Autopilot can never do" onClose={onClose} className="ap-dialog">
      {settings.status === "ready" ? <HardDeniedPage rules={settings.data.hardDenied} /> : <Loading label="Reading the list" rows={3} />}
    </Dialog>
  );
}
