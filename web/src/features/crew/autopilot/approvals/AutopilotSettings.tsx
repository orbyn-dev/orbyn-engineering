import { useRef, useState } from "react";
import { Loading, ErrorState } from "../../../../product/ui/states";
import { toFailure } from "../../../../lib/useAsync";
import { crewApi } from "./api";
import { MODE_COPY, TRIP_WIRES } from "./copy";
import { enforcedRules, HardDeniedList } from "./HardDenied";
import { MODES, type AutopilotSettings as Settings, type Mode } from "./model";
import { Chip } from "./parts";
import { useAutopilotSettings } from "./useCrew";

const HIGH_RISK = "Auth, keys, crypto, migrations, CI, dependency changes, large diffs, and areas or engines with a high revert rate. The risk of each task is shown with its reasons.";

/** Three cards, one choice: Off, On or Full. Arrow keys move the choice. */
export function ModePicker({ mode, onChange }: { mode: Mode; onChange(mode: Mode): void }) {
  const group = useRef<HTMLDivElement>(null);
  return (
    <div
      ref={group}
      className="ap-modes"
      role="radiogroup"
      aria-label="Autopilot"
      onKeyDown={(event) => {
        const step = event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 0;
        if (!step) return;
        event.preventDefault();
        const next = MODES[(MODES.indexOf(mode) + step + MODES.length) % MODES.length];
        if (!next) return;
        onChange(next);
        requestAnimationFrame(() => group.current?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
      }}
    >
      {MODES.map((one) => (
        <button key={one} type="button" role="radio" aria-checked={one === mode} tabIndex={one === mode ? 0 : -1} className="ap-mode" onClick={() => onChange(one)}>
          <b>
            {MODE_COPY[one].name}
            {one === "full" ? <span className="ap-default">Default</span> : null}
          </b>
          <small>{MODE_COPY[one].text}</small>
        </button>
      ))}
    </div>
  );
}

function ModeNotes({ mode }: { mode: Mode }) {
  if (mode === "full") {
    return (
      <section className="ap-box" aria-label="Trip wires that stop a task">
        <h4>Trip wires that stop a task</h4>
        <ul className="ap-list">{TRIP_WIRES.map((line) => <li key={line}>{line}</li>)}</ul>
        <p className="ap-muted">Nothing is lost when a task stops: its worktree is kept, and you get a Needs you item with a new brief. Stopping is amber, not red.</p>
      </section>
    );
  }
  if (mode === "on") {
    return (
      <section className="ap-box">
        <h4>What High risk means here</h4>
        <p>{HIGH_RISK}</p>
      </section>
    );
  }
  return (
    <section className="ap-box ap-box-plain">
      <h4>Off: agents will ask you</h4>
      <p>Every request an agent makes, and every merge, waits for you. Work on other tasks keeps going, and the Needs you inbox ranks what waits.</p>
    </section>
  );
}

function PerCrew({ settings }: { settings: Settings }) {
  return (
    <div className="ap-crews" role="table" aria-label="Autopilot per crew">
      <div className="ap-crew-row ap-head" role="row">
        {["Crew", "Autopilot", "Why"].map((name) => <span key={name} role="columnheader">{name}</span>)}
      </div>
      {settings.crews.map((crew) => (
        <div key={crew.crew} className="ap-crew-row" role="row">
          <span role="cell"><b>{crew.crew}</b><small>{crew.detail}</small></span>
          <span role="cell"><Chip tone="plain">{MODE_COPY[crew.mode].name}</Chip></span>
          <span role="cell" className="ap-clip">{crew.why}</span>
        </div>
      ))}
    </div>
  );
}

/** Settings › Crew › Autopilot: who answers agents’ requests, and what merges without waiting for you. */
export function AutopilotSettings() {
  const read = useAutopilotSettings();
  const [failed, setFailed] = useState<string | null>(null);
  if (read.status === "loading") return <Loading label="Reading Autopilot" rows={3} />;
  if (read.status === "error") return <ErrorState title="Can't read Autopilot" failure={read.error} onRetry={read.reload} />;
  const settings = read.data;
  const choose = (mode: Mode) => {
    if (mode === settings.mode) return;
    setFailed(null);
    const before = settings;
    read.setData({ ...settings, mode, confirmed: true });
    crewApi.setMode(mode).catch((error: unknown) => {
      read.setData(before);
      setFailed(toFailure(error).message);
    });
  };
  const hardCount = enforcedRules(settings.hardDenied).length;
  return (
    <section className="ap-set" aria-labelledby="ap-set-title">
      <header className="ap-set-head">
        <h2 id="ap-set-title">Autopilot</h2>
        <Chip tone="plain">Preview</Chip>
      </header>
      <p className="ap-lead">
        Who answers agents’ requests, and what merges without waiting for you. Applies to <b>{settings.repo}</b>
        {settings.crews.length ? " · all crews; a crew can override the repo." : "."}
      </p>
      <ModePicker mode={settings.mode} onChange={choose} />
      {failed ? <p role="alert" className="ap-error">{failed}</p> : null}
      <ModeNotes mode={settings.mode} />
      {settings.crews.length ? (
        <>
          <PerCrew settings={settings} />
          <details className="ap-fold">
            <summary>
              <b>Always denied, in every mode</b>
              <span>{hardCount} actions</span>
              <span className="ap-link">Show the list</span>
            </summary>
            <HardDeniedList rules={settings.hardDenied} heading="Always denied, in every mode" />
          </details>
        </>
      ) : (
        <HardDeniedList rules={settings.hardDenied} />
      )}
    </section>
  );
}
