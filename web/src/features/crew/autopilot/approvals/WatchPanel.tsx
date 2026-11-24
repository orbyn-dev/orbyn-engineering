import { ChevronLeft } from "lucide-react";
import { Button } from "../../../../ui/controls";
import type { BackgroundCrew as WireCrew } from "../../../../generated/BackgroundCrew";
import type { WatchReadRow } from "./backgroundModel";
import { Chip, Facts } from "./parts";
import { watchView, type WatchChangesView } from "./watchModel";

/**
 * One watch's page (Crew designs: watch-passing, watch-failing, watch-failing-template, watch-still-failing,
 * watch-recovered, watch-paused, watch-command-error and watch-history): its state, what the last check found, the facts
 * a check keeps, the redacted tail, and only the changes of result. It is drawn only from the watch's own read; a service
 * whose watch route answers 404 draws nothing of it (./useBackground drops that crew's row instead of inventing one).
 */
export function WatchChanges({ view }: { view: WatchChangesView }) {
  return (
    <section className="ap-block ap-watch-changes" aria-label="Changes">
      <header className="ap-set-head">
        <h3>Changes</h3>
        <span className="ap-muted">Only a change of result is listed.</span>
      </header>
      <div className="ap-bg" role="table" aria-label="Watch changes">
        <div className="ap-bg-row ap-head" role="row">
          {["When", "Change", "Exit", "What it did"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {view.rows.map((row) => (
          <div key={row.id} className="ap-bg-row" role="row">
            <span role="cell" className="ap-clip">{row.when}</span>
            <span role="cell">{row.change}</span>
            <span role="cell" className="ap-mono">{row.exit}</span>
            <span role="cell">{row.did}</span>
          </div>
        ))}
      </div>
      {view.note ? <p className="ap-bg-note">{view.note}</p> : null}
      {view.detail ? (
        <section className="ap-box ap-watch-output" aria-label="Change detail">
          <h4>{view.detail.title}</h4>
          <pre><code>{view.detail.body}</code></pre>
        </section>
      ) : null}
    </section>
  );
}

export interface WatchPanelProps {
  crew: WireCrew;
  watch: WatchReadRow;
  /** The project's name, as the "Runs in" fact shows it. */
  repo: string;
  /** The switch is being written for this crew. */
  busy?: boolean;
  onBack(): void;
  onToggle?(crew: WireCrew, enabled: boolean): void;
  /** Opens the Needs you inbox, where the row this change raised waits. */
  onOpenNeeds?(): void;
  /** Opens the run the newest change started, when the read holds that run. */
  onOpenRun?(): void;
}

export function WatchPanel({ crew, watch, repo, busy, onBack, onToggle, onOpenNeeds, onOpenRun }: WatchPanelProps) {
  const view = watchView(crew, watch.status, watch.changes, new Date(), repo);
  const changed = view.chip.tone === "am" || view.chip.tone === "no";
  return (
    <section className="ap-page ap-watch" aria-label={crew.name + " watch"}>
      <header className="ap-set-head ap-watch-head">
        <button type="button" className="ap-watch-back" onClick={onBack}>
          <ChevronLeft size={14} aria-hidden />
          Background crews
        </button>
        <b>{crew.name}</b>
        <Chip tone="plain">Watch</Chip>
        <Chip tone="plain">Preview</Chip>
        <Chip tone={view.chip.tone}>{view.chip.text}</Chip>
        <span className="ap-spacer" />
        <Chip tone={crew.enabled ? "ok" : "dim"}>{crew.enabled ? "On" : "Off"}</Chip>
        <Button small tone="quiet" disabled={!onToggle} busy={busy} aria-busy={busy || undefined} onClick={() => onToggle?.(crew, !crew.enabled)}>
          {crew.enabled ? "Turn off" : "Turn on"}
        </Button>
      </header>
      <div className={changed ? "ap-box ap-box-am ap-watch-banner" : "ap-box ap-watch-banner"}>
        <h4>{view.banner.title}</h4>
        <p>{view.banner.text}</p>
        {view.banner.action === "needs_you" ? <Button small disabled={!onOpenNeeds} onClick={onOpenNeeds}>Open in Needs you</Button> : null}
        {/* Drawn only when the read holds the run it opens: a button with no run would say what it does not do (#1354). */}
        {view.banner.action === "run" && onOpenRun ? <Button small onClick={onOpenRun}>Open the run</Button> : null}
      </div>
      <div className="ap-box ap-watch-facts">
        <Facts rows={view.facts} />
      </div>
      {view.tail ? (
        <section className="ap-box ap-watch-output" aria-label="Latest output">
          <h4>{view.tail.title}</h4>
          <pre><code>{view.tail.body}</code></pre>
        </section>
      ) : null}
      <WatchChanges view={view.changes} />
    </section>
  );
}
