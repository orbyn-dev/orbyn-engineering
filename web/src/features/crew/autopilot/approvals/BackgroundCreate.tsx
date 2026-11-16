import { cloneElement, useState, type ReactElement } from "react";
import { toFailure } from "../../../../lib/useAsync";
import { Button } from "../../../../ui/controls";
import { Dialog } from "../../../../ui/Dialog";
import { Segmented } from "../../../../ui/Segmented";
import type { BackgroundTrigger } from "../../../../generated/BackgroundTrigger";
import { crewApi } from "./api";
import { DEFAULT_BUDGET, LIMITS, WIRE_TEMPLATE, WIRE_TRIGGER } from "./backgroundModel";
import { TEMPLATES } from "./copy";
import type { TemplateId, TriggerId } from "./model";
import { Chip } from "./parts";

/** The four choices the dialog draws for "Starts when", in the words it uses. */
const STARTS: { id: TriggerId; label: string }[] = [
  { id: "ci_fails", label: "CI fails" },
  { id: "issue_label", label: "An issue gets a label" },
  { id: "schedule", label: "On a schedule" },
  { id: "dependency_update", label: "A dependency update opens" },
];

export type Field = "name" | "workflow" | "branch" | "label" | "hours" | "turns" | "minutes";

/** The field a route refusal is about, from its message, or null when it is about none of them. */
export function fieldOf(message: string): Field | null {
  const text = message.toLowerCase();
  if (/budget.*turns/.test(text)) return "turns";
  if (/budget.*minutes/.test(text)) return "minutes";
  if (/^name\b|that name/.test(text)) return "name";
  if (/^label\b|needs label/.test(text)) return "label";
  if (/every_hours/.test(text)) return "hours";
  if (/^workflow\b/.test(text)) return "workflow";
  if (/^branch\b/.test(text)) return "branch";
  return null;
}

const whole = (text: string, max: number): number | null => {
  const value = Number(text);
  return text.trim() !== "" && Number.isInteger(value) && value >= 1 && value <= max ? value : null;
};

function Entry({ id, label, hint, error, children }: { id: string; label: string; hint?: string; error?: string; children: ReactElement<{ "aria-describedby"?: string }> }) {
  const described = error ? `${id}-error` : hint ? `${id}-hint` : undefined;
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {cloneElement(children, { "aria-describedby": described })}
      {error ? <span className="field-error" id={`${id}-error`} role="alert">{error}</span> : hint ? <span className="field-hint" id={`${id}-hint`}>{hint}</span> : null}
    </div>
  );
}

/** Crew designs `background-new`, as ruled: a short form, saved on this device. The crew starts off; turning it on is its own step on the list. */
export function BackgroundCreate({ project, template: first, onClose, onCreated }: { project: string; template?: TemplateId; onClose(): void; onCreated(): void }) {
  const [template, setTemplate] = useState<TemplateId>(first ?? "fix_ci");
  const nameOf = (id: TemplateId) => TEMPLATES.find((one) => one.id === id)?.name ?? id;
  const triggerOf = (id: TemplateId): TriggerId => TEMPLATES.find((one) => one.id === id)?.trigger ?? "ci_fails";
  const [starts, setStarts] = useState<TriggerId>(() => triggerOf(first ?? "fix_ci"));
  const [name, setName] = useState(() => nameOf(first ?? "fix_ci"));
  const [named, setNamed] = useState(false);
  const [workflow, setWorkflow] = useState("");
  const [branch, setBranch] = useState("");
  const [label, setLabel] = useState("");
  const [hours, setHours] = useState("24");
  const [turns, setTurns] = useState(String(DEFAULT_BUDGET.turns));
  const [minutes, setMinutes] = useState(String(DEFAULT_BUDGET.minutes));
  const [busy, setBusy] = useState(false);
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});
  const [general, setGeneral] = useState<string | null>(null);

  /** An edit to a field drops the route's refusal of it. */
  const clear = (field: Field) => setErrors((now) => (now[field] ? { ...now, [field]: undefined } : now));
  const choose = (next: TemplateId) => {
    setTemplate(next);
    setStarts(triggerOf(next));
    if (!named) setName(nameOf(next));
  };
  const turnsN = whole(turns, LIMITS.turns);
  const minutesN = whole(minutes, LIMITS.minutes);
  const hoursN = whole(hours, LIMITS.hours);
  const trigger = (): BackgroundTrigger => {
    if (starts === "ci_fails") return { kind: "ci_failure", workflow: workflow.trim() || null, branch: branch.trim() || null };
    if (starts === "issue_label") return { kind: "issue_label", label };
    if (starts === "schedule") return { kind: "schedule", every_hours: hoursN };
    return { kind: WIRE_TRIGGER[starts] };
  };
  const ready = name.trim() !== "" && turnsN !== null && minutesN !== null && (starts !== "issue_label" || label.trim() !== "") && (starts !== "schedule" || hoursN !== null);

  const create = async () => {
    if (!ready || turnsN === null || minutesN === null) return;
    setBusy(true);
    setErrors({});
    setGeneral(null);
    try {
      await crewApi.backgroundCreate({ project_id: project, name, template: WIRE_TEMPLATE[template], trigger: trigger(), budget: { turns: turnsN, minutes: minutesN } });
      onCreated();
      onClose();
    } catch (failure) {
      const message = toFailure(failure).message;
      const field = fieldOf(message);
      if (field) setErrors({ [field]: message });
      else setGeneral(message);
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={<>New background crew <Chip tone="plain">Preview</Chip></>}
      className="ap-dialog ap-bg-new"
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>Cancel</Button>
          <Button tone="primary" busy={busy} disabled={!ready} onClick={() => void create()}>Create background crew</Button>
        </>
      }
    >
      <div className="form">
        <p className="ap-muted">It starts when the trigger fires, runs the full flow and lands in your Review queue. It is kept on this device.</p>
        <div className="field">
          <label>Template</label>
          <Segmented label="Template" value={template} onChange={choose} options={TEMPLATES.map((one) => ({ value: one.id, label: one.name }))} />
        </div>
        <Entry id="bg-name" label="Name" hint={`1 to ${LIMITS.name} characters`} error={errors.name}>
          <input id="bg-name" value={name} aria-invalid={errors.name ? true : undefined} onChange={(event) => { setName(event.target.value); setNamed(true); clear("name"); }} />
        </Entry>
        <div className="field">
          <label>Starts when <Chip tone="plain">Not built yet</Chip></label>
          <Segmented<TriggerId> label="Starts when" value={starts} onChange={setStarts} options={STARTS.map((one) => ({ value: one.id, label: one.label }))} />
        </div>
        {starts === "ci_fails" ? (
          <div className="ap-pair">
            <Entry id="bg-workflow" label="CI workflow" error={errors.workflow}>
              <input id="bg-workflow" className="ap-mono" value={workflow} placeholder="Any workflow" onChange={(event) => { setWorkflow(event.target.value); clear("workflow"); }} />
            </Entry>
            <Entry id="bg-branch" label="Branch" error={errors.branch}>
              <input id="bg-branch" className="ap-mono" value={branch} placeholder="Any branch" onChange={(event) => { setBranch(event.target.value); clear("branch"); }} />
            </Entry>
          </div>
        ) : null}
        {starts === "issue_label" ? (
          <Entry id="bg-label" label="Label" hint={`1 to ${LIMITS.text} characters`} error={errors.label}>
            <input id="bg-label" className="ap-mono" value={label} maxLength={LIMITS.text} onChange={(event) => { setLabel(event.target.value); clear("label"); }} />
          </Entry>
        ) : null}
        {starts === "schedule" ? (
          <Entry id="bg-hours" label="Hours between runs" hint={`1 to ${LIMITS.hours} hours`} error={errors.hours}>
            <input id="bg-hours" type="number" min={1} max={LIMITS.hours} value={hours} onChange={(event) => { setHours(event.target.value); clear("hours"); }} />
          </Entry>
        ) : null}
        <div className="field">
          <label id="bg-budget">Budget for each run</label>
          <div className="ap-pair" role="group" aria-labelledby="bg-budget">
          <Entry id="bg-turns" label="Turns for each run" hint={`1 to ${LIMITS.turns}`} error={errors.turns}>
            <input id="bg-turns" type="number" min={1} max={LIMITS.turns} value={turns} onChange={(event) => { setTurns(event.target.value); clear("turns"); }} />
          </Entry>
          <Entry id="bg-minutes" label="Minutes for each run" hint={`1 to ${LIMITS.minutes}`} error={errors.minutes}>
            <input id="bg-minutes" type="number" min={1} max={LIMITS.minutes} value={minutes} onChange={(event) => { setMinutes(event.target.value); clear("minutes"); }} />
          </Entry>
          </div>
        </div>
        {general ? <p className="field-error" role="alert">{general}</p> : null}
      </div>
    </Dialog>
  );
}
