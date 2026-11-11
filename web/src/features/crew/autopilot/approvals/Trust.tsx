import { ErrorState, Loading } from "../../../../product/ui/states";
import { useAsync } from "../../../../lib/useAsync";
import { crewApi } from "./api";
import type { ReviewerRow, SoloRow, TrustCell, TrustRow, TrustView } from "./model";

/** A rate shows only from this many merges (or findings, or runs). Below it the cell says so. */
export const MIN_FOR_A_RATE = 10;
const MIN_SOLO = 5;

const percent = (cell: TrustCell) => Math.round((cell.count / cell.of) * 100);

function Rate({ count, of, word }: { count: number; of: number; word: string }) {
  const rated = of >= MIN_FOR_A_RATE;
  return (
    <span className="ap-rate">
      <b>{count} of {of}</b>
      <small>{rated ? `${word} · ${percent({ count, of })}%` : "too few for a rate"}</small>
    </span>
  );
}

function RateTable({ title, first, rows }: { title: string; first: string; rows: TrustRow[] }) {
  return (
    <section className="ap-block" aria-label={title}>
      <h3>{title}</h3>
      <div className="ap-trust" role="table" aria-label={title}>
        <div className="ap-trust-row ap-head" role="row">
          {[first, "Reverted", "Reworked", "Sample"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {rows.map((row) => (
          <div key={row.name} className="ap-trust-row" role="row">
            <span role="cell"><b>{row.name}</b>{row.vendor ? <small>{row.vendor}</small> : null}</span>
            <span role="cell"><Rate {...row.reverted} word="reverted" /></span>
            <span role="cell"><Rate {...row.reworked} word="reworked" /></span>
            <span role="cell">{row.merges} merges</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function Reviewers({ rows }: { rows: ReviewerRow[] }) {
  return (
    <section className="ap-block" aria-label="Trust · reviewers">
      <h3>Trust · reviewers</h3>
      <p className="ap-lead">Each reviewer lane’s precision: how many of its confirmed findings were real and how many the Finding check dropped, with the sample. It feeds which reviewers are paired and which noisy finding types are muted, per repo and on this device. Below {MIN_FOR_A_RATE} findings the cell says it is too few for a rate.</p>
      <div className="ap-trust ap-trust-rev" role="table" aria-label="Reviewers">
        <div className="ap-trust-row ap-head" role="row">
          {["Reviewer", "Confirmed", "Dropped", "Sample"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {rows.map((row) => (
          <div key={row.name} className="ap-trust-row" role="row">
            <span role="cell"><b>{row.name}</b></span>
            <span role="cell">
              <span className="ap-rate">
                <b>{row.confirmed} of {row.findings}</b>
                <small>
                  {row.findings >= MIN_FOR_A_RATE
                    ? `confirmed findings were real · ${Math.round((row.confirmed / row.findings) * 100)}%`
                    : "confirmed were real · too few for a rate"}
                </small>
              </span>
            </span>
            <span role="cell">
              <span className="ap-rate"><b>{row.dropped} of {row.findings}</b>{row.findings >= MIN_FOR_A_RATE ? <small>dropped by the Finding check</small> : null}</span>
            </span>
            <span role="cell">{row.findings} findings</span>
          </div>
        ))}
      </div>
      <section className="ap-box">
        <h4>How this is used</h4>
        <p>A reviewer whose findings are often dropped is asked for fewer low-severity findings. Nothing changes without showing it here before it happens.</p>
      </section>
    </section>
  );
}

const sideLine = (s: { reverted: number; reworked: number; medianMin: number; cost: string }) => `${s.reverted} reverted · ${s.reworked} reworked · median ${s.medianMin} min · ${s.cost}`;

function CrewVsSolo({ rows }: { rows: SoloRow[] }) {
  return (
    <section className="ap-block" aria-label="Crew vs solo">
      <h3>Crew vs solo</h3>
      <p className="ap-lead">Crew has to earn its cost. For each task class the page puts Crew’s merges beside solo runs (one engine, no crew) on the same kind of task: reverts, rework, time and cost, each with its sample. Where there aren’t enough solo runs it says “not enough solo runs to say”. It never claims Crew is better when the sample can’t show it.</p>
      <div className="ap-trust ap-solo" role="table" aria-label="Crew vs solo">
        <div className="ap-trust-row ap-head" role="row">
          {["Task class", "Crew", "Solo, one engine", "What it shows"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {rows.map((row) => {
          const enough = row.solo.runs >= MIN_SOLO && row.crew.merges >= MIN_SOLO;
          return (
            <div key={row.taskClass} className="ap-trust-row" role="row">
              <span role="cell"><b>{row.taskClass}</b></span>
              <span role="cell"><span className="ap-rate"><b>{row.crew.merges} merges</b><small>{sideLine(row.crew)}</small></span></span>
              <span role="cell"><span className="ap-rate"><b>{row.solo.runs} runs</b><small>{enough ? sideLine(row.solo) : "too few to compare"}</small></span></span>
              <span role="cell">{enough && row.shows ? row.shows : "Not enough solo runs to say"}</span>
            </div>
          );
        })}
      </div>
      <section className="ap-box">
        <h4>How to read this</h4>
        <p>A solo run is a task done by one engine with no other reviewer or worker. A comparison needs at least {MIN_SOLO} runs on each side. The router uses what it learns here from {MIN_FOR_A_RATE} comparisons up.</p>
      </section>
    </section>
  );
}

export function TrustPageView({ view }: { view: TrustView }) {
  const young = view.merges < MIN_FOR_A_RATE;
  return (
    <section className="ap-page" aria-label="Trust">
      <header className="ap-set-head">
        <h2>Trust</h2>
        <span className="ap-sub">{view.repo} · last {view.days} days · this device</span>
      </header>
      {young ? (
        <p className="ap-banner ap-banner-plain" role="status">
          This repo has {view.merges} merges. A rate shows from {MIN_FOR_A_RATE} merges, over the last {view.days} days, per repo on this device; until then Crew shows the individual results and the router uses defaults.
        </p>
      ) : (
        <p className="ap-lead">Revert and rework rate per engine, per area of the repo and per reviewer, shown to you and used by the router and the risk tier. Every figure carries its sample size, and a rate is shown only from {MIN_FOR_A_RATE} merges. It is per repo, on this device, over the last {view.days} days.</p>
      )}
      <RateTable title="By engine" first="Engine" rows={view.engines} />
      {view.areas.length ? <RateTable title="By area" first="Area" rows={view.areas} /> : null}
      {view.reviewers.length ? <Reviewers rows={view.reviewers} /> : null}
      {view.solo.length ? <CrewVsSolo rows={view.solo} /> : null}
    </section>
  );
}

export function TrustTab() {
  const read = useAsync("crew-trust", (signal) => crewApi.trust(signal), { keep: true });
  if (read.status === "loading") return <Loading label="Reading trust" rows={4} />;
  if (read.status === "error") return <ErrorState title="Can't read trust" failure={read.error} onRetry={read.reload} />;
  return <TrustPageView view={read.data} />;
}
