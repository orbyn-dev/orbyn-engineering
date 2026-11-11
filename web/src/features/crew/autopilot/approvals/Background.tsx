import { useRef, useState } from "react";
import { Button } from "../../../../ui/controls";
import { Dialog } from "../../../../ui/Dialog";
import { Segmented } from "../../../../ui/Segmented";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { toFailure, useAsync } from "../../../../lib/useAsync";
import { crewApi } from "./api";
import { MODE_COPY, TEMPLATES, TRIGGERS } from "./copy";
import type { BackgroundCrew, BackgroundRun, BackgroundView, NewBackgroundCrew, TemplateId, TriggerId } from "./model";
import { Chip } from "./parts";

const templateName = (id: TemplateId) => TEMPLATES.find((one) => one.id === id)?.short ?? id;
const DETAIL_LABEL: Record<TriggerId, string> = { ci_fails: "CI workflow and branch", issue_label: "Label", schedule: "Schedule", dependency_update: "Updates from" };
const DETAIL_DEFAULT: Record<TriggerId, string> = { ci_fails: "ci.yml on main", issue_label: "crew-fix", schedule: "Every weekday at 07:00", dependency_update: "Dependabot or Renovate PRs" };

function CrewRows({ crews, onToggle }: { crews: BackgroundCrew[]; onToggle(crew: BackgroundCrew, enabled: boolean): void }) {
  return (
    <div className="ap-bg" role="table" aria-label="Background crews">
      <div className="ap-bg-row ap-head" role="row">
        {["Crew", "Starts when", "Last run", "State", "Autopilot"].map((name) => <span key={name} role="columnheader">{name}</span>)}
      </div>
      {crews.map((crew) => (
        <div key={crew.id} className="ap-bg-row" role="row">
          <span role="cell"><b>{crew.name}</b><small>Template: {templateName(crew.template)}</small></span>
          <span role="cell" className="ap-clip">{crew.startsWhen}</span>
          <span role="cell" className="ap-clip">{crew.lastRun ?? "Never run"}</span>
          <span role="cell">
            <button type="button" role="switch" aria-checked={crew.enabled} aria-label={`${crew.name} state`} className="ap-switch" onClick={() => onToggle(crew, !crew.enabled)}>
              {crew.enabled ? "On" : "Off"}
            </button>
          </span>
          <span role="cell"><Chip tone="plain">{MODE_COPY[crew.mode].name}</Chip></span>
        </div>
      ))}
    </div>
  );
}

/** The 320 px side panel: each background crew is a small card with its trigger and state. */
export function BackgroundCards({ crews, onOpen }: { crews: BackgroundCrew[]; onOpen?(crew: BackgroundCrew): void }) {
  return (
    <ul className="ap-cards" aria-label="Background crews">
      {crews.map((crew) => (
        <li key={crew.id}>
          <button type="button" className="ap-card" onClick={() => onOpen?.(crew)}>
            <span className="ap-card-top"><b>{crew.name}</b><Chip tone="plain">{crew.enabled ? "On" : "Off"}</Chip></span>
            <small>{crew.startsWhen}</small>
          </button>
        </li>
      ))}
    </ul>
  );
}

function Runs({ runs }: { runs: BackgroundRun[] }) {
  return (
    <section className="ap-block" aria-label="Recent runs">
      <header className="ap-set-head"><h3>Recent runs</h3><Chip tone="plain">Preview</Chip></header>
      <div className="ap-bg ap-runs" role="table" aria-label="Recent runs">
        <div className="ap-bg-row ap-head" role="row">
          {["Run", "Started by", "Result", "State", "Autopilot"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {runs.map((run) => (
          <div key={run.id} className="ap-bg-row" role="row">
            <span role="cell"><b>{run.name}</b><small>Template: {templateName(run.template)}</small></span>
            <span role="cell" className="ap-clip">{run.startedBy}</span>
            <span role="cell" className="ap-clip">{run.result}</span>
            <span role="cell"><Chip tone={run.state === "Stopped" ? "am" : run.state === "Failed" ? "no" : run.state === "Merged" ? "ok" : "dim"}>{run.state}</Chip></span>
            <span role="cell"><Chip tone="plain">{MODE_COPY[run.mode].name}</Chip></span>
          </div>
        ))}
      </div>
    </section>
  );
}

function Templates({ onUse }: { onUse(id: TemplateId): void }) {
  return (
    <section className="ap-block" id="ap-templates" aria-label="Templates">
      <header className="ap-set-head"><h3>Templates</h3><Chip tone="plain">Preview</Chip></header>
      <div className="ap-templates">
        {TEMPLATES.map((template) => (
          <article key={template.id} className="ap-box" aria-label={template.name}>
            <h4>{template.name}</h4>
            <p className="ap-muted">Starts when: {template.starts}</p>
            <p>{template.does}</p>
            <Button small onClick={() => onUse(template.id)} aria-label={`Use the ${template.name} template`}>Use this template</Button>
          </article>
        ))}
      </div>
    </section>
  );
}

function Triggers() {
  return (
    <section className="ap-block" aria-label="Triggers">
      <header className="ap-set-head"><h3>Triggers</h3><Chip tone="plain">Preview</Chip></header>
      <div className="ap-bg ap-trig" role="table" aria-label="Triggers">
        <div className="ap-bg-row ap-head" role="row">
          {["Starts when", "Configured as", "How repeats are handled"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {TRIGGERS.map((trigger) => (
          <div key={trigger.id} className="ap-bg-row" role="row">
            <span role="cell"><b>{trigger.name}</b></span>
            <span role="cell">{trigger.configured}</span>
            <span role="cell">{trigger.repeats}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

export function NewBackgroundDialog({ initial, defaultBudget, onCreate, onClose }: { initial: TemplateId; defaultBudget: string; onCreate(crew: NewBackgroundCrew): Promise<void>; onClose(): void }) {
  const [template, setTemplate] = useState<TemplateId>(initial);
  const [trigger, setTrigger] = useState<TriggerId>(TEMPLATES.find((one) => one.id === initial)?.trigger ?? "ci_fails");
  const [detail, setDetail] = useState(DETAIL_DEFAULT[trigger]);
  const [budget, setBudget] = useState(defaultBudget);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const chooseTrigger = (next: TriggerId) => {
    setTrigger(next);
    setDetail(DETAIL_DEFAULT[next]);
  };
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await onCreate({ template, trigger, detail, budget });
      onClose();
    } catch (failure) {
      setError(toFailure(failure).message);
      setBusy(false);
    }
  };
  return (
    <Dialog
      title="Create a background crew"
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy}>Cancel</Button>
          <Button tone="primary" onClick={() => void submit()} busy={busy}>Create background crew</Button>
        </>
      }
    >
      <p>It starts when the trigger fires, runs the full flow and lands in your Review queue. It is saved in .orbyn/crew.json so it can be reviewed like any other change.</p>
      <div className="ap-field">
        <Segmented<TemplateId> label="Template" value={template} onChange={setTemplate} options={TEMPLATES.map((one) => ({ value: one.id, label: one.name }))} />
      </div>
      <div className="ap-field">
        <Segmented<TriggerId> label="Starts when" value={trigger} onChange={chooseTrigger} options={[
          { value: "ci_fails", label: "CI fails" },
          { value: "issue_label", label: "An issue gets a label" },
          { value: "schedule", label: "On a schedule" },
          { value: "dependency_update", label: "A dependency update opens" },
        ]} />
      </div>
      <label className="ap-field"><span>{DETAIL_LABEL[trigger]}</span><input value={detail} onChange={(event) => setDetail(event.target.value)} /></label>
      <label className="ap-field"><span>Budget for each run</span><input value={budget} onChange={(event) => setBudget(event.target.value)} /></label>
      {error ? <p role="alert" className="ap-error">{error}</p> : null}
    </Dialog>
  );
}

export function BackgroundPageView({ view, onToggle, onCreate }: { view: BackgroundView; onToggle(crew: BackgroundCrew, enabled: boolean): void; onCreate(crew: NewBackgroundCrew): Promise<void> }) {
  const [creating, setCreating] = useState<TemplateId | null>(null);
  const templates = useRef<HTMLDivElement>(null);
  const none = view.crews.length === 0;
  return (
    <section className="ap-page" aria-label="Background crews">
      <header className="ap-set-head">
        <h2>Background crews{none ? "" : <span className="ap-num"> {view.crews.length}</span>}</h2>
        <Chip tone="plain">Preview</Chip>
        <span className="ap-spacer" />
        <code className="ap-file">.orbyn/crew.json</code>
        <Button small tone="primary" onClick={() => setCreating("fix_ci")}>New background crew</Button>
      </header>
      {none ? (
        <Empty title="No background crews yet">
          <p>A background crew starts when something happens in the repo, like CI failing or a dependency update, runs the whole flow, and lands its change in your Review queue.</p>
          <Button small onClick={() => templates.current?.querySelector<HTMLElement>("button")?.focus()}>See the templates</Button>
        </Empty>
      ) : (
        <>
          <CrewRows crews={view.crews} onToggle={onToggle} />
          <section className="ap-box">
            <h4>Every run lands in the Review queue</h4>
            <p>A background crew never merges outside the flow: its changes are checked, reviewed and merged like any other, and you review them after the merge. Its budget is set when the crew is created.</p>
          </section>
          {view.runs.length ? <Runs runs={view.runs} /> : null}
        </>
      )}
      <div ref={templates}><Templates onUse={setCreating} /></div>
      <Triggers />
      {creating ? <NewBackgroundDialog initial={creating} defaultBudget={view.defaultBudget} onCreate={onCreate} onClose={() => setCreating(null)} /> : null}
    </section>
  );
}

export function BackgroundTab() {
  const read = useAsync("crew-background", (signal) => crewApi.background(signal), { keep: true });
  if (read.status === "loading") return <Loading label="Reading background crews" rows={4} />;
  if (read.status === "error") return <ErrorState title="Can't read background crews" failure={read.error} onRetry={read.reload} />;
  return (
    <BackgroundPageView
      view={read.data}
      onToggle={(crew, enabled) => {
        const before = read.data;
        read.setData({ ...before, crews: before.crews.map((one) => (one.id === crew.id ? { ...one, enabled } : one)) });
        crewApi.setBackground(crew.id, enabled).catch(() => read.setData(before));
      }}
      onCreate={async (crew) => {
        await crewApi.createBackground(crew);
        read.reload();
      }}
    />
  );
}
