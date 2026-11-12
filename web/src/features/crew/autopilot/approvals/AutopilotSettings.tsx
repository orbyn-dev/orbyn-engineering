import { useRef, useState } from "react";
import { Loading, ErrorState } from "../../../../product/ui/states";
import { toFailure } from "../../../../lib/useAsync";
import { crewApi } from "./api";
import { MODE_COPY } from "./copy";
import type { AutopilotGuards } from "../../../../generated/AutopilotGuards";
import type { AutopilotModeEntry } from "../../../../generated/AutopilotModeEntry";
import { HardDeniedList, listed } from "./HardDenied";
import { dayOf } from "./ledger";
import { MODES, type Mode } from "./model";
import { Chip } from "./parts";
import { useAutopilotGuards, useAutopilotModes } from "./useCrew";

const HIGH_RISK = "Auth, keys, crypto, migrations, CI, dependency changes, large diffs, and areas or engines with a high revert rate. The risk of each task is shown with its reasons.";

/** Three cards, one choice: Off, On or Full. Arrow keys move the choice and stop at the ends. */
export function ModePicker({ mode, onChange, disabled = false }: { mode: Mode; onChange(mode: Mode): void; disabled?: boolean }) {
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
        const next = MODES[MODES.indexOf(mode) + step];
        if (!next || disabled) return;
        onChange(next);
        requestAnimationFrame(() => group.current?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
      }}
    >
      {MODES.map((one) => (
        <button key={one} type="button" role="radio" aria-checked={one === mode} aria-disabled={disabled || undefined} tabIndex={one === mode ? 0 : -1} className="ap-mode" onClick={() => !disabled && onChange(one)}>
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

function ModeNotes({ mode, guards }: { mode: Mode; guards: AutopilotGuards }) {
  if (mode === "full") {
    return (
      <section className="ap-box" aria-label="Trip wires that stop a task">
        <h4>Trip wires that stop a task</h4>
        <ul className="ap-list">
          <li>{guards.in_a_row_limit} denials in a row stop the task with the reason “too many denials”.</li>
          <li>{guards.in_a_run_limit} denials in one run stop it with the same reason.</li>
          <li>Text that looks like injected instructions, in a page or a file the worker read, stops it with the reason “possible injected instructions”.</li>
        </ul>
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

function PerCrew({ entries }: { entries: AutopilotModeEntry[] }) {
  return (
    <div className="ap-crews" role="table" aria-label="Autopilot per crew">
      <div className="ap-crew-row ap-head" role="row">
        {["Crew", "Autopilot", "Why"].map((name) => <span key={name} role="columnheader">{name}</span>)}
      </div>
      {entries.map((entry) => (
        <div key={`${entry.scope}-${entry.id}`} className="ap-crew-row" role="row">
          <span role="cell"><b>{entry.id}</b></span>
          <span role="cell"><Chip tone="plain">{MODE_COPY[entry.mode].name}</Chip></span>
          <span role="cell" className="ap-clip">You set this {entry.scope === "crew" ? "crew" : "repo"} to {MODE_COPY[entry.mode].name} on {dayOf(entry.updated_at)}.</span>
        </div>
      ))}
    </div>
  );
}

/** Settings › Crew › Autopilot: who answers agents’ requests, and what merges without waiting for you. */
export function AutopilotSettings() {
  const modes = useAutopilotModes();
  const guards = useAutopilotGuards();
  const [failed, setFailed] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const latest = useRef(0);
  const inflight = useRef(false);
  if (modes.status === "loading" || guards.status === "loading") return <Loading label="Reading Autopilot" rows={3} />;
  if (modes.status === "error") return <ErrorState title="Can't read Autopilot" failure={modes.error} onRetry={modes.reload} />;
  if (guards.status === "error") return <ErrorState title="Can't read Autopilot" failure={guards.error} onRetry={guards.reload} />;
  const mode = modes.data.entries.find((entry) => entry.scope === "default")?.mode ?? modes.data.effective_default;
  const overrides = modes.data.entries.filter((entry) => entry.scope !== "default");
  const choose = (next: Mode) => {
    if (next === mode || inflight.current) return;
    inflight.current = true;
    setFailed(null);
    setBusy(true);
    const mine = ++latest.current;
    // The new choice shows at once. Only the newest answer is applied; a refusal shows the server's own state again.
    modes.setData({ ...modes.data, effective_default: next, confirmed: true, entries: [{ scope: "default", mode: next, updated_at: new Date().toISOString() }, ...modes.data.entries.filter((entry) => entry.scope !== "default")] });
    crewApi.setMode(next).then(
      (reply) => {
        if (mine === latest.current) modes.setData(reply);
      },
      (error: unknown) => {
        if (mine !== latest.current) return;
        setFailed(toFailure(error).message);
        void modes.reloadNow();
      },
    ).finally(() => {
      inflight.current = false;
      if (mine === latest.current) setBusy(false);
    });
  };
  return (
    <section className="ap-set" aria-labelledby="ap-set-title">
      <header className="ap-set-head">
        <h2 id="ap-set-title">Autopilot</h2>
        <Chip tone="plain">Preview</Chip>
      </header>
      <p className="ap-lead">
        Who answers agents’ requests, and what merges without waiting for you. Applies to every repo
        {overrides.length ? " · all crews; a crew can override the repo." : "; a crew can override the repo."}
      </p>
      <ModePicker mode={mode} onChange={choose} disabled={busy} />
      {failed ? <p role="alert" className="ap-error">{failed}</p> : null}
      <ModeNotes mode={mode} guards={guards.data} />
      {overrides.length ? (
        <>
          <PerCrew entries={overrides} />
          <details className="ap-fold">
            <summary>
              <b>Always denied, in every mode</b>
              <span>{listed(guards.data.always_denied).length} actions</span>
              <span className="ap-link">Show the list</span>
            </summary>
            <HardDeniedList rules={guards.data.always_denied} heading="The list" />
          </details>
        </>
      ) : (
        <HardDeniedList rules={guards.data.always_denied} />
      )}
    </section>
  );
}
