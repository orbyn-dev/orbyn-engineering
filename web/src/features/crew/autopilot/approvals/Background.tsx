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

function CrewRows({ crews, onToggle, busy }: { crews: BackgroundCrew[]; onToggle?(crew: BackgroundCrew, enabled: boolean): void; busy?: ReadonlySet<string> }) {
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
          <span role="cell" className="ap-state">
            {/* aria-disabled, not disabled: the switch keeps focus while its write is in flight. */}
            <button type="button" role="switch" aria-checked={crew.enabled} aria-label={`${crew.name} state`} className="ap-switch" disabled={!onToggle} aria-disabled={busy?.has(crew.id) || undefined} onClick={() => { if (!busy?.has(crew.id)) onToggle?.(crew, !crew.enabled); }}>
              {crew.enabled ? "On" : "Off"}
            </button>
            {crew.status ? <small>{crew.status}</small> : null}
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
    <>
    <Chip tone="plain">Planned</Chip>
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
    </>
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
            <span role="cell">{run.state ? <Chip tone={run.state === "Stopped" ? "am" : run.state === "Failed" ? "no" : run.state === "Merged" ? "ok" : run.state === "Needs review" ? "plain" : "dim"}>{run.reason ? `${run.state} · ${run.reason}` : run.state}</Chip> : null}</span>
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
  /** The crews whose switch is being written. */
  busy?: ReadonlySet<string>;
  /** The repository the crews belong to, drawn as a plain label. */
  repo?: string;
  /** The page has no git project: nothing is listed, and Create and the templates say why they are off. */
  noRepo?: boolean;
  /** A line above the list, such as the read stopping before the end. */
  notice?: string;
  /** Opens the create dialog; without it the page draws its button disabled. */
  onCreate?(): void;
  onUseTemplate?(template: TemplateId): void;
}

export function BackgroundPageView({ view, onToggle, busy, repo, noRepo, notice, onCreate, onUseTemplate }: PageProps) {
  const templates = useRef<HTMLDivElement>(null);
  const none = view.crews.length === 0;
  return (
    <section className="ap-page" aria-label="Background crews">
      {repo ? <p className="ap-muted ap-repo">Repo: {repo}</p> : null}
      {noRepo ? (
        <Empty title="No git project">
          <p>Background crews belong to a git project, so Create and the templates are off until one is open.</p>
        </Empty>
      ) : none ? (
        <Empty title="No background crews yet">
          <p>A background crew starts when something happens in the repo, like CI failing or a dependency update, runs the whole flow, and lands its change in your Review queue. Triggers are not built yet.</p>
          <Button small onClick={() => templates.current?.scrollIntoView?.({ block: "start" })}>See the templates</Button>
        </Empty>
      ) : (
        <>
          <header className="ap-set-head">
            <h2>Background crews<span className="ap-num"> {view.total ?? view.crews.length}</span></h2>
            <Chip tone="plain">Preview</Chip>
            <Chip tone="plain">This device</Chip>
            <Chip tone="plain">Triggers not built yet</Chip>
            <span className="ap-spacer" />
            <Button small tone="primary" icon={<Plus size={14} aria-hidden />} disabled={!onCreate} onClick={onCreate}>New background crew</Button>
          </header>
          {notice ? <p className="ap-muted" role="status">{notice}</p> : null}
          <CrewRows crews={view.crews} onToggle={onToggle} busy={busy} />
          {hasMode(view.crews) ? null : <p className="ap-bg-note">Per-crew Autopilot is Planned.</p>}
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

/** Background crews of the repo the page is about, read from the Crew routes. Without a git project nothing is read or listed. */
export function BackgroundTab() {
  const repo = useRepo();
  if (repo === "loading") return <Loading label="Reading background crews" rows={4} />;
  if (!repo) return <BackgroundPageView view={{ crews: [], runs: [] }} noRepo />;
  return <Background project={repo.id} repo={repo.name} />;
}

function Background({ project, repo }: { project: string; repo: string }) {
  const read = useBackground(project);
  const [creating, setCreating] = useState<{ template?: TemplateId } | null>(null);
  const [busy, setBusy] = useState<ReadonlySet<string>>(new Set());
  const [refused, setRefused] = useState<string | null>(null);
  if (read.status === "loading") return <Loading label="Reading background crews" rows={4} />;
  if (!read.data) return <ErrorState title="Can’t read background crews" failure={read.error!} onRetry={() => void read.reload()} />;
  const view = backgroundView(read.data, new Date());
  const toggle = (crew: BackgroundCrew, enabled: boolean) => {
    const mark = (on: boolean) => setBusy((now) => { const next = new Set(now); if (on) next.add(crew.id); else next.delete(crew.id); return next; });
    mark(true);
    setRefused(null);
    void (enabled ? crewApi.backgroundEnable(crew.id) : crewApi.backgroundDisable(crew.id))
      .then(() => read.reload(), (failure: unknown) => setRefused(toFailure(failure).message))
      .finally(() => mark(false));
  };
  const more = read.data.total > read.data.crews.length;
  return (
    <>
      {refused ? <p className="ap-error" role="alert">{refused}</p> : null}
      {/* A read that failed after one that worked keeps the crews on screen and says so. */}
      {read.status === "error" ? <ErrorState compact title="Can’t refresh background crews" failure={read.error} onRetry={() => void read.reload()} /> : null}
      <BackgroundPageView
        view={view}
        repo={repo}
        notice={more ? `Showing ${read.data.crews.length} of ${read.data.total} background crews.` : undefined}
        onToggle={toggle}
        busy={busy}
        onCreate={() => setCreating({})}
        onUseTemplate={(template) => setCreating({ template })}
      />
      {creating ? <BackgroundCreate project={project} template={creating.template} onClose={() => setCreating(null)} onCreated={() => void read.reload()} /> : null}
    </>
  );
}
