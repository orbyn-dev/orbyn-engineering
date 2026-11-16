import { ErrorState, Loading } from "../../../../product/ui/states";
import { useAsync } from "../../../../lib/useAsync";
import type { AutopilotRate } from "../../../../generated/AutopilotRate";
import type { AutopilotRateRow } from "../../../../generated/AutopilotRateRow";
import type { AutopilotTrust } from "../../../../generated/AutopilotTrust";
import { crewApi } from "./api";

/** Reverts per thousand merges, as the percent a person reads. */
const percent = (rate: AutopilotRate) => Math.round(rate.revert_permille / 10);

/** A rate shows only once the sample is big enough (`counts`); until then the cell says so. */
function Rate({ rate }: { rate: AutopilotRate }) {
  return (
    <span className="ap-rate">
      <b>{rate.reverts} of {rate.merges}</b>
      <small>{rate.counts ? `reverted · ${percent(rate)}%` : "too few for a rate"}</small>
    </span>
  );
}

function RateTable({ title, first, rows }: { title: string; first: string; rows: AutopilotRateRow[] }) {
  return (
    <section className="ap-block" aria-label={title}>
      <h3>{title}</h3>
      <div className="ap-trust" role="table" aria-label={title}>
        <div className="ap-trust-row ap-head" role="row">
          {[first, "Reverted", "Sample"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {rows.map((row) => (
          <div key={row.name} className="ap-trust-row" role="row">
            <span role="cell"><b>{row.name}</b></span>
            <span role="cell"><Rate rate={row.rate} /></span>
            <span role="cell">{row.rate.merges} merges</span>
          </div>
        ))}
      </div>
    </section>
  );
}

export function TrustPageView({ view }: { view: AutopilotTrust }) {
  const young = !view.total.counts;
  return (
    <section className="ap-page" aria-label="Trust">
      <header className="ap-set-head">
        <h2>Trust</h2>
        <span className="ap-sub">last {view.window_days} days · this device</span>
      </header>
      {young ? (
        <p className="ap-banner ap-banner-plain" role="status">
          This repo has {view.total.merges} merges. A rate shows from {view.min_sample} merges, over the last {view.window_days} days, per repo on this device; until then Crew shows the individual results and the router uses defaults.
        </p>
      ) : (
        <p className="ap-lead">Revert rate per engine and per area of the repo, shown to you and used by the router and the risk tier. Every figure carries its sample size, and a rate is shown only from {view.min_sample} merges. It is per repo, on this device, over the last {view.window_days} days.</p>
      )}
      <RateTable title="By engine" first="Engine" rows={view.engines} />
      {view.areas.length ? <RateTable title="By area" first="Area" rows={view.areas} /> : null}
      {view.truncated ? <p className="ap-muted">More areas and engines are tracked than listed.</p> : null}
    </section>
  );
}

export function TrustTab() {
  const read = useAsync("crew-trust", (signal) => crewApi.trust(signal), { keep: true });
  if (read.status === "loading") return <Loading label="Reading trust" rows={4} />;
  if (read.status === "error") return <ErrorState title="Can't read trust" failure={read.error} onRetry={read.reload} />;
  return <TrustPageView view={read.data} />;
}
