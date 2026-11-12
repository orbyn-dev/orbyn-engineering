import { useRef } from "react";
import { Button } from "../../../../ui/controls";
import { Empty } from "../../../../product/ui/states";
import { NOT_CONNECTED } from "./api";
import { MODE_COPY, TEMPLATES, TRIGGERS } from "./copy";
import type { Mode, TemplateId } from "./model";
import { Chip } from "./parts";

// Background crews have no route yet: their state will live in the app's data folder, not in the repo.
// The shapes below are what the screens draw once a route serves them.
export interface BackgroundCrew {
  id: string;
  name: string;
  template: TemplateId;
  startsWhen: string;
  lastRun: string | null;
  enabled: boolean;
  mode: Mode;
}
export interface BackgroundRun {
  id: string;
  name: string;
  template: TemplateId;
  startedBy: string;
  result: string;
  state: "Merged" | "Running" | "Stopped" | "Failed";
  mode: Mode;
}
export interface BackgroundView {
  crews: BackgroundCrew[];
  runs: BackgroundRun[];
  /** The default budget for a new crew, in each engine's own units. */
  defaultBudget: string;
}

const templateName = (id: TemplateId) => TEMPLATES.find((one) => one.id === id)?.short ?? id;

function CrewRows({ crews, onToggle }: { crews: BackgroundCrew[]; onToggle?(crew: BackgroundCrew, enabled: boolean): void }) {
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
            <button type="button" role="switch" aria-checked={crew.enabled} aria-label={`${crew.name} state`} className="ap-switch" disabled={!onToggle} onClick={() => onToggle?.(crew, !crew.enabled)}>
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
      <div className="ap-bg" role="table" aria-label="Recent runs">
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

function Templates() {
  return (
    <section className="ap-block" id="ap-templates" aria-label="Templates">
      <header className="ap-set-head"><h3>Templates</h3><Chip tone="plain">Preview</Chip></header>
      <div className="ap-templates">
        {TEMPLATES.map((template) => (
          <article key={template.id} className="ap-box" aria-label={template.name}>
            <h4>{template.name}</h4>
            <p className="ap-muted">Starts when: {template.starts}</p>
            <p>{template.does}</p>
            <Button small disabled title={NOT_CONNECTED} aria-description={NOT_CONNECTED} aria-label={`Use the ${template.name} template`}>Use this template</Button>
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
            <span role="cell" className="ap-muted">Not set</span>
            <span role="cell">{trigger.repeats}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

export function BackgroundPageView({ view, onToggle }: { view: BackgroundView; onToggle?(crew: BackgroundCrew, enabled: boolean): void }) {
  const templates = useRef<HTMLDivElement>(null);
  const none = view.crews.length === 0;
  return (
    <section className="ap-page" aria-label="Background crews">
      <header className="ap-set-head">
        <h2>Background crews{none ? "" : <span className="ap-num"> {view.crews.length}</span>}</h2>
        <Chip tone="plain">Preview</Chip>
        <span className="ap-spacer" />
        <code className="ap-file">.orbyn/crew.json</code>
        <Button small tone="primary" disabled title={NOT_CONNECTED} aria-description={NOT_CONNECTED}>New background crew</Button>
      </header>
      {none ? (
        <Empty title="No background crews yet">
          <p>A background crew starts when something happens in the repo, like CI failing or a dependency update, runs the whole flow, and lands its change in your Review queue.</p>
          <Button small onClick={() => templates.current?.scrollIntoView?.({ block: "start" })}>See the templates</Button>
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
      <div ref={templates}><Templates /></div>
      <Triggers />
    </section>
  );
}

/** Nothing serves background crews yet, so the page is the empty one, with the templates and triggers it will use. */
export function BackgroundTab() {
  return (
    <>
      <BackgroundPageView view={{ crews: [], runs: [], defaultBudget: "" }} />
      <p className="ap-muted" role="note">{NOT_CONNECTED}.</p>
    </>
  );
}
