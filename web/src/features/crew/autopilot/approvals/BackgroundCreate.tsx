import { useState } from "react";
import { ConfirmDialog } from "../../../../ui/Dialog";
import { Segmented } from "../../../../ui/Segmented";
import type { BackgroundTrigger } from "../../../../generated/BackgroundTrigger";
import { crewApi } from "./api";
import { DEFAULT_BUDGET, WIRE_TEMPLATE, WIRE_TRIGGER, budgetLine } from "./backgroundModel";
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
/** The designs draw no field for the label of an issue trigger or for the hours of a schedule, and the route needs both: those two wait for the designer. */
const NEEDS_FIELD: TriggerId[] = ["issue_label", "schedule"];

/** "ci.yml on main", "ci.yml" or "on main" as the workflow and branch of the CI trigger; empty means any. */
export function ciTrigger(line: string): BackgroundTrigger {
  const text = line.trim();
  const match = /^(?:(.*?)\s+)?on\s+(\S.*)$/.exec(text);
  const workflow = (match ? match[1] ?? "" : text).trim();
  return { kind: "ci_failure", workflow: workflow || null, branch: match?.[2]?.trim() || null };
}

/** Crew designs `background-new`: a short form, saved on this device. The crew starts off; turning it on is its own step on the list. */
export function BackgroundCreate({ project, template: first, onClose, onCreated }: { project: string; template?: TemplateId; onClose(): void; onCreated(): void }) {
  const [template, setTemplate] = useState<TemplateId>(first ?? "fix_ci");
  const [starts, setStarts] = useState<TriggerId>(() => TEMPLATES.find((one) => one.id === (first ?? "fix_ci"))?.trigger ?? "ci_fails");
  const [ci, setCi] = useState("");
  const choose = (next: TemplateId) => {
    setTemplate(next);
    setStarts(TEMPLATES.find((one) => one.id === next)?.trigger ?? "ci_fails");
  };
  const trigger: BackgroundTrigger = starts === "ci_fails" ? ciTrigger(ci) : { kind: WIRE_TRIGGER[starts] };
  const name = TEMPLATES.find((one) => one.id === template)?.name ?? template;
  return (
    <ConfirmDialog
      title={<>New background crew <Chip tone="plain">Preview</Chip></>}
      className="ap-dialog ap-bg-new"
      confirmLabel="Create background crew"
      confirmDisabled={NEEDS_FIELD.includes(starts)}
      onClose={onClose}
      onConfirm={async () => {
        await crewApi.backgroundCreate({ project_id: project, name, template: WIRE_TEMPLATE[template], trigger, budget: DEFAULT_BUDGET });
        onCreated();
      }}
    >
      <div className="form">
        <p className="ap-muted">It starts when the trigger fires, runs the full flow and lands in your Review queue. It is kept on this device.</p>
        <div className="field">
          <label>Template</label>
          <Segmented label="Template" value={template} onChange={choose} options={TEMPLATES.map((one) => ({ value: one.id, label: one.name }))} />
        </div>
        <div className="field">
          <label>Starts when <Chip tone="plain">Not built yet</Chip></label>
          <Segmented<TriggerId> label="Starts when" value={starts} onChange={setStarts} options={STARTS.map((one) => ({ value: one.id, label: one.label, disabled: NEEDS_FIELD.includes(one.id) }))} />
        </div>
        {starts === "ci_fails" ? (
          <div className="field">
            <label htmlFor="bg-ci">CI workflow and branch</label>
            <input id="bg-ci" className="ap-mono" value={ci} placeholder="ci.yml on main" onChange={(event) => setCi(event.target.value)} />
          </div>
        ) : null}
        <div className="field">
          <label htmlFor="bg-budget">Budget for each run</label>
          <input id="bg-budget" readOnly value={budgetLine(DEFAULT_BUDGET)} />
        </div>
      </div>
    </ConfirmDialog>
  );
}
