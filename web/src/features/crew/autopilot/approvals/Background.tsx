import { Plus, Workflow } from "lucide-react";
import { useRef, useState } from "react";
import { toFailure } from "../../../../lib/useAsync";
import { Button } from "../../../../ui/controls";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { useRepo } from "./AutonomyAreas";
import { BackgroundCreate } from "./BackgroundCreate";
import { backgroundView, type BackgroundCrew, type BackgroundRun, type BackgroundView } from "./backgroundModel";

export type { BackgroundCrew, BackgroundRun, BackgroundView } from "./backgroundModel";
import { crewApi } from "./api";
import { MODE_COPY, TEMPLATES, TRIGGERS } from "./copy";
import type { Mode, TemplateId, TriggerId } from "./model";
import { Chip } from "./parts";
import { useBackground } from "./useBackground";

const templateName = (id: TemplateId) => TEMPLATES.find((one) => one.id === id)?.short ?? id;
const hasMode = (rows: { mode: Mode | null }[]) => rows.length > 0 && rows.every((row) => row.mode !== null);

function CrewRows({ crews, onToggle, busy }: { crews: BackgroundCrew[]; onToggle?(crew: BackgroundCrew, enabled: boolean): void; busy?: string | null }) {
  const mode = hasMode(crews);
  return (
    <div className={mode ? "ap-bg" : "ap-bg ap-bg-solo"} role="table" aria-label="Background crews">
      <div className="ap-bg-row ap-head" role="row">
        {["Crew", "Starts when", "Last run", "State", ...(mode ? ["Autopilot"] : [])].map((name) => <span key={name} role="columnheader">{name}</span>)}
      </div>
      {crews.map((crew) => (
        <div key={crew.id} className="ap-bg-row" role="row">
          <span role="cell"><b>{crew.name}</b><small>Template: {templateName(crew.template)}</small></span>
          <span role="cell" className="ap-clip">{crew.startsWhen}</span>
          <span role="cell" className="ap-clip">{crew.lastRun ?? "Never run"}</span>
          <span role="cell">
            <button type="button" role="switch" aria-checked={crew.enabled} aria-label={`${crew.name} state`} className="ap-switch" disabled={!onToggle || busy === crew.id} onClick={() => onToggle?.(crew, !crew.enabled)}>
              {crew.enabled ? "On" : "Off"}
            </button>
          </span>
          {mode && crew.mode ? <span role="cell"><Chip tone="plain">{MODE_COPY[crew.mode].name}</Chip></span> : null}
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
  const mode = hasMode(runs);
  return (
    <section className="ap-block" aria-label="Recent runs">
      <header className="ap-set-head"><h3>Recent runs</h3><Chip tone="plain">Preview</Chip></header>
      <div className={mode ? "ap-bg" : "ap-bg ap-bg-solo"} role="table" aria-label="Recent runs">
        <div className="ap-bg-row ap-head" role="row">
          {["Run", "Started by", "Result", "State", ...(mode ? ["Autopilot"] : [])].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {runs.map((run) => (
          <div key={run.id} className="ap-bg-row" role="row">
            <span role="cell"><b>{run.name}</b><small>Template: {templateName(run.template)}</small></span>
            <span role="cell" className="ap-clip">{run.startedBy}</span>
            <span role="cell" className="ap-clip">{run.result}</span>
            <span role="cell">{run.state ? <Chip tone={run.state === "Stopped" ? "am" : run.state === "Failed" ? "no" : run.state === "Merged" ? "ok" : "dim"}>{run.state}</Chip> : null}</span>
            {mode && run.mode ? <span role="cell"><Chip tone="plain">{MODE_COPY[run.mode].name}</Chip></span> : null}
          </div>
        ))}
      </div>
    </section>
  );
}

function Templates({ onUse }: { onUse?(template: TemplateId): void }) {
  return (
    <section className="ap-block" id="ap-templates" aria-label="Templates">
      <header className="ap-set-head"><h3>Templates</h3><Chip tone="plain">Preview</Chip><Chip tone="plain">Triggers not built yet</Chip></header>
      <div className="ap-templates">
        {TEMPLATES.map((template) => (
          <article key={template.id} className="ap-box" aria-label={template.name}>
            <h4><Workflow size={14} aria-hidden />{template.name}</h4>
            <p className="ap-muted">Starts when: {template.starts}</p>
            <p>{template.does}</p>
            <Button small disabled={!onUse} onClick={() => onUse?.(template.id)} aria-label={`Use the ${template.name} template`}>Use this template</Button>
          </article>
        ))}
      </div>
    </section>
  );
}

function Triggers({ configured }: { configured?: Partial<Record<TriggerId, string>> }) {
  return (
    <section className="ap-block" aria-label="Triggers">
      <header className="ap-set-head"><h3>Triggers</h3><Chip tone="plain">Preview</Chip><Chip tone="plain">Not built yet</Chip></header>
      <div className="ap-bg ap-trig" role="table" aria-label="Triggers">
        <div className="ap-bg-row ap-head" role="row">
          {["Starts when", "Configured as", "How repeats are handled"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {TRIGGERS.map((trigger) => (
          <div key={trigger.id} className="ap-bg-row" role="row">
            <span role="cell"><b>{trigger.name}</b></span>
            <span role="cell" className={configured?.[trigger.id] ? "ap-file" : "ap-muted"}>{configured?.[trigger.id] ?? "Not set"}</span>
            <span role="cell">{trigger.repeats}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

interface PageProps {
  view: BackgroundView;
  onToggle?(crew: BackgroundCrew, enabled: boolean): void;
  /** The crew whose switch is being written. */
  busy?: string | null;
  /** Opens the create dialog; without it the page draws its button disabled. */
  onCreate?(): void;
  onUseTemplate?(template: TemplateId): void;
}

export function BackgroundPageView({ view, onToggle, busy, onCreate, onUseTemplate }: PageProps) {
  const templates = useRef<HTMLDivElement>(null);
  const none = view.crews.length === 0;
  return (
    <section className="ap-page" aria-label="Background crews">
      {none ? (
        <Empty title="No background crews yet">
          <p>A background crew starts when something happens in the repo, like CI failing or a dependency update, runs the whole flow, and lands its change in your Review queue. Triggers are not built yet.</p>
          <Button small onClick={() => templates.current?.scrollIntoView?.({ block: "start" })}>See the templates</Button>
        </Empty>
      ) : (
        <>
          <header className="ap-set-head">
            <h2>Background crews<span className="ap-num"> {view.crews.length}</span></h2>
            <Chip tone="plain">Preview</Chip>
            <Chip tone="plain">This device</Chip>
            <Chip tone="plain">Triggers not built yet</Chip>
            <span className="ap-spacer" />
            <Button small tone="primary" icon={<Plus size={14} aria-hidden />} disabled={!onCreate} onClick={onCreate}>New background crew</Button>
          </header>
          <CrewRows crews={view.crews} onToggle={onToggle} busy={busy} />
          <section className="ap-box">
            <h4>Every run lands in the Review queue</h4>
            <p>A background crew never merges outside the flow: its changes are checked, reviewed and merged like any other, and you review them after the merge. Its budget is set when the crew is created.</p>
          </section>
          {view.runs.length ? <Runs runs={view.runs} /> : null}
        </>
      )}
      <div ref={templates}><Templates onUse={onUseTemplate} /></div>
      <Triggers configured={view.configured} />
    </section>
  );
}

/** Background crews of the repo the page is about, read from the Crew routes. */
export function BackgroundTab() {
  const repo = useRepo();
  if (repo === "loading") return <Loading label="Reading background crews" rows={4} />;
  return <Background project={repo?.id} />;
}

function Background({ project }: { project: string | undefined }) {
  const read = useBackground(project);
  const [creating, setCreating] = useState<{ template?: TemplateId } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  if (read.status === "loading") return <Loading label="Reading background crews" rows={4} />;
  if (!read.data) return <ErrorState title="Can’t read background crews" failure={read.error!} onRetry={() => void read.reload()} />;
  const view = backgroundView(read.data, new Date());
  const toggle = (crew: BackgroundCrew, enabled: boolean) => {
    setBusy(crew.id);
    setRefused(null);
    void (enabled ? crewApi.backgroundEnable(crew.id) : crewApi.backgroundDisable(crew.id))
      .then(() => read.reload(), (failure: unknown) => setRefused(toFailure(failure).message))
      .finally(() => setBusy(null));
  };
  return (
    <>
      {refused ? <p className="ap-error" role="alert">{refused}</p> : null}
      <BackgroundPageView view={view} onToggle={toggle} busy={busy} onCreate={project ? () => setCreating({}) : undefined} onUseTemplate={project ? (template) => setCreating({ template }) : undefined} />
      {creating && project ? <BackgroundCreate project={project} template={creating.template} onClose={() => setCreating(null)} onCreated={() => void read.reload()} /> : null}
    </>
  );
}
