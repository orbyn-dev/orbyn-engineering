import { cloneElement, useState, type ReactElement } from "react";
import { toFailure } from "../../../../lib/useAsync";
import { Button } from "../../../../ui/controls";
import { Dialog } from "../../../../ui/Dialog";
import { Segmented } from "../../../../ui/Segmented";
import type { BackgroundTrigger } from "../../../../generated/BackgroundTrigger";
import { crewApi } from "./api";
import { DEFAULT_BUDGET, DEFAULT_DAILY, LIMITS, WIRE_TEMPLATE, WIRE_TRIGGER } from "./backgroundModel";
import { TEMPLATES } from "./copy";
import type { TemplateId, TriggerId } from "./model";
import { Chip } from "./parts";

/** The five choices the dialog draws for "Starts when", in the words it uses. */
const STARTS: { id: TriggerId; label: string }[] = [
  { id: "ci_fails", label: "CI fails" },
  { id: "issue_label", label: "An issue gets a label" },
  { id: "schedule", label: "On a schedule" },
  { id: "dependency_update", label: "A dependency update opens" },
  { id: "watch", label: "Watch" },
];

/** What a watch does when a check turns failing: raise one Needs you row, or start its template's run. */
type OnFailing = "needs_you" | "template";
const ON_FAILING: { id: OnFailing; label: string }[] = [
  { id: "needs_you", label: "Raise Needs you" },
  { id: "template", label: "Start the crew template" },
];

export type Field = "name" | "workflow" | "branch" | "label" | "hours" | "command" | "turns" | "minutes" | "dailyRuns" | "dailyTurns";

/** The field a route refusal is about, from its message, or null when it is about none of them. */
export function fieldOf(message: string): Field | null {
  const text = message.toLowerCase();
  // The daily budget's refusals are checked first: they also match the per-run budget's words.
  if (/daily budget.*runs/.test(text)) return "dailyRuns";
  if (/daily budget.*turns/.test(text)) return "dailyTurns";
  if (/budget.*turns/.test(text)) return "turns";
  if (/budget.*minutes/.test(text)) return "minutes";
  if (/^name\b|that name/.test(text)) return "name";
  if (/^label\b|needs label/.test(text)) return "label";
  // A watch command: the range, a hidden character and the gate's own refusal all name the command.
  if (/^command\b|that command|needs command/.test(text)) return "command";
  if (/every_hours/.test(text)) return "hours";
  if (/^workflow\b/.test(text)) return "workflow";
  if (/^branch\b/.test(text)) return "branch";
  return null;
}

const whole = (text: string, max: number): number | null => {
  const value = Number(text);
  return text.trim() !== "" && Number.isInteger(value) && value >= 1 && value <= max ? value : null;
};

/** The sentence under a field while its current value is outside the range the frame draws, in the app's limits. */
const RANGE = {
  name: `Use 1 to ${LIMITS.name} characters`,
  label: `Use 1 to ${LIMITS.text} characters`,
  command: `Use 1 to ${LIMITS.command} characters`,
  hours: `Use 1 to ${LIMITS.hours} hours`,
  turns: `Use 1 to ${LIMITS.turns} turns`,
  minutes: `Use 1 to ${LIMITS.minutes} minutes`,
  dailyRuns: `Use 1 to ${LIMITS.dailyRuns} runs a day`,
  dailyTurns: `Use 1 to ${LIMITS.dailyTurns} turns a day`,
} as const;

const textRange = (text: string, max: number, message: string): string | undefined => {
  const length = text.trim().length;
  return length >= 1 && length <= max ? undefined : message;
};
const numberRange = (value: number | null, message: string): string | undefined => (value === null ? message : undefined);

/** `hintId` points the field at a hint drawn once for several fields, such as the budget's. */
function Entry({ id, label, hint, hintId, error, children }: { id: string; label: string; hint?: string; hintId?: string; error?: string; children: ReactElement<{ "aria-describedby"?: string; "aria-invalid"?: true }> }) {
  const described = error ? `${id}-error` : hint ? `${id}-hint` : hintId;
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {cloneElement(children, { "aria-describedby": described, "aria-invalid": error ? true : undefined })}
      {error ? <span className="field-error" id={`${id}-error`} role="alert">{error}</span> : hint ? <span className="field-hint" id={`${id}-hint`}>{hint}</span> : null}
    </div>
  );
}

/** Crew designs `background-new` and `watch-setup`, as ruled: a short form, saved on this device. The crew starts off; turning it on is its own step on the list. */
export function BackgroundCreate({ project, template: first, onClose, onCreated }: { project: string; template?: TemplateId; onClose(): void; onCreated(): void }) {
  const [template, setTemplate] = useState<TemplateId>(first ?? "fix_ci");
  const nameOf = (id: TemplateId) => TEMPLATES.find((one) => one.id === id)?.name ?? id;
  const triggerOf = (id: TemplateId): TriggerId => TEMPLATES.find((one) => one.id === id)?.trigger ?? "ci_fails";
  const [starts, setStarts] = useState<TriggerId>(() => triggerOf(first ?? "fix_ci"));
  const [onFailing, setOnFailing] = useState<OnFailing>("needs_you");
  const [name, setName] = useState(() => nameOf(first ?? "fix_ci"));
  const [named, setNamed] = useState(false);
  const [workflow, setWorkflow] = useState("");
  const [branch, setBranch] = useState("");
  const [label, setLabel] = useState("");
  const [command, setCommand] = useState("");
  const [hours, setHours] = useState("24");
  const [turns, setTurns] = useState(String(DEFAULT_BUDGET.turns));
  const [minutes, setMinutes] = useState(String(DEFAULT_BUDGET.minutes));
  const [dailyRuns, setDailyRuns] = useState(String(DEFAULT_DAILY.runs));
  const [dailyTurns, setDailyTurns] = useState(String(DEFAULT_DAILY.turns));
  const [busy, setBusy] = useState(false);
  const [errors, setErrors] = useState<Partial<Record<Field, string>>>({});
  const [general, setGeneral] = useState<string | null>(null);

  const watch = starts === "watch";
  /** A watch that only raises a Needs you row starts no run, so its budget is never asked and the defaults go with it. */
  const asksBudget = !watch || onFailing === "template";

  /** An edit to a field drops the route's refusal of it. */
  const clear = (field: Field) => setErrors((now) => (now[field] ? { ...now, [field]: undefined } : now));
  const choose = (next: TemplateId) => {
    setTemplate(next);
    setStarts(triggerOf(next));
    if (!named) setName(nameOf(next));
  };
  const turnsN = whole(turns, LIMITS.turns);
  const minutesN = whole(minutes, LIMITS.minutes);
  const dailyRunsN = whole(dailyRuns, LIMITS.dailyRuns);
  const dailyTurnsN = whole(dailyTurns, LIMITS.dailyTurns);
  const hoursN = whole(hours, LIMITS.hours);
  const nameRange = textRange(name, LIMITS.name, RANGE.name);
  const labelRange = textRange(label, LIMITS.text, RANGE.label);
  const commandRange = watch ? textRange(command, LIMITS.command, RANGE.command) : undefined;
  const turnsError = errors.turns ?? numberRange(turnsN, RANGE.turns);
  const minutesError = errors.minutes ?? numberRange(minutesN, RANGE.minutes);
  const dailyRunsError = errors.dailyRuns ?? numberRange(dailyRunsN, RANGE.dailyRuns);
  const dailyTurnsError = errors.dailyTurns ?? numberRange(dailyTurnsN, RANGE.dailyTurns);
  const nameError = errors.name ?? nameRange;
  const hoursError = errors.hours ?? numberRange(hoursN, RANGE.hours);
  const labelError = errors.label ?? labelRange;
  const commandError = errors.command ?? commandRange;
  const budgetReady = !asksBudget || (turnsN !== null && minutesN !== null && dailyRunsN !== null && dailyTurnsN !== null);
  const trigger = (): BackgroundTrigger => {
    if (starts === "ci_fails") return { kind: "ci_failure", workflow: workflow.trim() || null, branch: branch.trim() || null };
    if (starts === "issue_label") return { kind: "issue_label", label };
    if (starts === "schedule") return { kind: "schedule", every_hours: hoursN };
    if (starts === "watch") return { kind: "watch", command, every_hours: hoursN, on_failing: onFailing };
    return { kind: WIRE_TRIGGER[starts] };
  };
  const ready = nameRange === undefined && budgetReady && (starts !== "issue_label" || labelRange === undefined) && (starts !== "schedule" || hoursN !== null) && (!watch || (commandRange === undefined && hoursN !== null));

  const create = async () => {
    if (!ready) return;
    setBusy(true);
    setErrors({});
    setGeneral(null);
    try {
      // A watch that raises a Needs you row never spends a model budget: the contract still takes one, so the defaults go.
      const budget = asksBudget && turnsN !== null && minutesN !== null ? { turns: turnsN, minutes: minutesN } : DEFAULT_BUDGET;
      const daily = asksBudget && dailyRunsN !== null && dailyTurnsN !== null ? { runs: dailyRunsN, turns: dailyTurnsN } : DEFAULT_DAILY;
      // The person's clock goes with the daily budget, the way a scheduled task sends it, so the budget resets at their midnight.
      await crewApi.backgroundCreate({ project_id: project, name, template: WIRE_TEMPLATE[template], trigger: trigger(), budget, daily, utc_offset_minutes: -new Date().getTimezoneOffset() });
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
        <p className="ap-muted">
          {watch
            ? "It is created off. Turn it on when you want it to start checking. It is kept on this device. A project can have at most 10 watches."
            : "It is created off. Turn it on when you want its trigger to start runs. It is kept on this device."}
        </p>
        <Entry id="bg-name" label="Name" hint={`1 to ${LIMITS.name} characters`} error={nameError}>
          <input id="bg-name" value={name} onChange={(event) => { setName(event.target.value); setNamed(true); clear("name"); }} />
        </Entry>
        {asksBudget ? (
          <div className="field">
            <label>Template</label>
            <Segmented label="Template" value={template} onChange={choose} options={TEMPLATES.map((one) => ({ value: one.id, label: one.name }))} />
          </div>
        ) : null}
        <div className="field">
          <label>Starts when</label>
          <Segmented<TriggerId> label="Starts when" value={starts} onChange={setStarts} options={STARTS.map((one) => ({ value: one.id, label: one.label }))} />
        </div>
        {starts === "ci_fails" ? (
          <div className="ap-pair">
            <Entry id="bg-workflow" label="CI workflow · optional" error={errors.workflow}>
              <input id="bg-workflow" className="ap-mono" value={workflow} placeholder="ci.yml" onChange={(event) => { setWorkflow(event.target.value); clear("workflow"); }} />
            </Entry>
            <Entry id="bg-branch" label="Branch · optional" error={errors.branch}>
              <input id="bg-branch" className="ap-mono" value={branch} placeholder="main" onChange={(event) => { setBranch(event.target.value); clear("branch"); }} />
            </Entry>
          </div>
        ) : null}
        {starts === "issue_label" ? (
          <Entry id="bg-label" label="Label" hint={`1 to ${LIMITS.text} characters. An issue that gets this label starts a run.`} error={labelError}>
            <input id="bg-label" className="ap-mono" value={label} onChange={(event) => { setLabel(event.target.value); clear("label"); }} />
          </Entry>
        ) : null}
        {watch ? (
          <Entry
            id="bg-command"
            label="Command"
            hint={`1 to ${LIMITS.command} characters. Runs in the project’s checkout root on this device with no model call and no network, and is cut off after 5 minutes (“could not run: timed out”). Exit 0 is passing, any other exit is failing. It goes through the same always-denied and blast-radius screen as a re-run check, and anything that looks like a key or token is hidden before it is saved.`}
            error={commandError}
          >
            <input id="bg-command" className="ap-mono" value={command} onChange={(event) => { setCommand(event.target.value); clear("command"); }} />
          </Entry>
        ) : null}
        {starts === "schedule" || watch ? (
          <Entry id="bg-hours" label="Every N hours" hint={watch ? "1 to 720. The shortest is every 1 hour, so at most 24 checks a day." : "1 to 720."} error={hoursError}>
            <input id="bg-hours" type="number" min={1} max={LIMITS.hours} value={hours} onChange={(event) => { setHours(event.target.value); clear("hours"); }} />
          </Entry>
        ) : null}
        {watch ? (
          <div className="field">
            <label id="bg-failing-label">When it turns failing</label>
            <Segmented<OnFailing> label="When it turns failing" value={onFailing} onChange={setOnFailing} options={ON_FAILING.map((one) => ({ value: one.id, label: one.label }))} />
            <span className="field-hint">
              {onFailing === "template" ? "Starts one run of the template each time the result turns failing, not while it stays failing." : "One row in Needs you each time the result turns failing. Nothing starts."}
            </span>
          </div>
        ) : null}
        {asksBudget ? (
          <>
            <div className="ap-pair">
              <Entry id="bg-turns" label="Turns" hintId="bg-budget-hint" error={turnsError}>
                <input id="bg-turns" type="number" min={1} max={LIMITS.turns} value={turns} onChange={(event) => { setTurns(event.target.value); clear("turns"); }} />
              </Entry>
              <Entry id="bg-minutes" label="Minutes" hintId="bg-budget-hint" error={minutesError}>
                <input id="bg-minutes" type="number" min={1} max={LIMITS.minutes} value={minutes} onChange={(event) => { setMinutes(event.target.value); clear("minutes"); }} />
              </Entry>
            </div>
            <span className="field-hint" id="bg-budget-hint">Budget for each run. Turns 1 to {LIMITS.turns}, minutes 1 to {LIMITS.minutes}. It is set now and does not change after.</span>
            <div className="ap-pair">
              <Entry id="bg-daily-runs" label="Runs a day" hintId="bg-daily-hint" error={dailyRunsError}>
                <input id="bg-daily-runs" type="number" min={1} max={LIMITS.dailyRuns} value={dailyRuns} onChange={(event) => { setDailyRuns(event.target.value); clear("dailyRuns"); }} />
              </Entry>
              <Entry id="bg-daily-turns" label="Turns a day" hintId="bg-daily-hint" error={dailyTurnsError}>
                <input id="bg-daily-turns" type="number" min={1} max={LIMITS.dailyTurns} value={dailyTurns} onChange={(event) => { setDailyTurns(event.target.value); clear("dailyTurns"); }} />
              </Entry>
            </div>
            <span className="field-hint" id="bg-daily-hint">Daily budget for each engine. Runs 1 to {LIMITS.dailyRuns}, turns 1 to {LIMITS.dailyTurns}. It is set now and does not change after, and it resets at your midnight.</span>
          </>
        ) : null}
        {watch ? (
          <section className="ap-box" aria-label={onFailing === "template" ? "The check has no model budget" : "No model call, no model budget"}>
            <h4>{onFailing === "template" ? "The check has no model budget" : "No model call, no model budget"}</h4>
            <p>
              {onFailing === "template"
                ? "A check is a command, not a model call: it spends no turns and none of an engine’s daily budget. Only a run it starts spends the budget above. The first check after you turn it on is a baseline: it is recorded and never raises, even if it fails."
                : "A check is a command, not a model call, so it spends no turns and none of an engine’s daily budget. It does nothing while the result stays the same. The first check after you turn it on is a baseline: it is recorded and never raises, even if it fails."}
            </p>
          </section>
        ) : null}
        {general ? <p className="field-error" role="alert">{general}</p> : null}
      </div>
    </Dialog>
  );
}
