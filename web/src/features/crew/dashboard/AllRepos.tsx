import { useWaited } from "../../../lib/motion";
import type { ApiFailure } from "../../../api/client";
import { Loading } from "../../../product/ui/states";
import { Failure } from "./Failure";
import type { DashboardClient, Period } from "./contract";
import { Planned, WHY } from "./parts";
import type { Choosable } from "./Controls";
import { costText, costTotals, costsOf, duration, enginesLine, medianOf, mergesOf, rate, text, type DashModel } from "./model";
import { MAX_REPOS, useEveryRepo, type RepoRead } from "./useDashboard";

const GRID = "minmax(0,1.3fr) minmax(0,1fr) minmax(0,1fr) minmax(0,1fr) minmax(0,1.5fr) auto";

function Autonomy({ model }: { model: DashModel }) {
  if (model.merges === 0) return <><b>0 merges</b><small>nothing to measure yet</small></>;
  if (model.unaided === null) return <Planned why={WHY.autonomy} />;
  const r = rate(model.unaided, model.merges, model.minRate);
  return r.pct ? <><b>{r.pct}</b><small>{r.of}</small></> : <><b>{r.of}</b><small>{r.few}</small></>;
}

function Cycle({ model }: { model: DashModel }) {
  if (model.merges === 0) return <><b>—</b><small>no merges</small></>;
  if (model.source === "trust") return <Planned why={WHY.cycle} />;
  const median = medianOf(model);
  return median === null ? <><b>Not measured</b><small>{mergesOf(model.merges)}</small></> : <><b>{duration(median)}</b><small>{mergesOf(model.merges)}</small></>;
}

function Reverts({ model }: { model: DashModel }) {
  if (model.merges === 0) return <><b>—</b><small>no merges</small></>;
  const r = rate(model.reverted, model.merges, model.minRate);
  return r.pct ? <><b>{r.pct}</b><small>{r.of}</small></> : <><b>{r.of}</b><small>{r.few}</small></>;
}

function Cost({ model }: { model: DashModel }) {
  if (model.merges === 0) return <><b>—</b><small>no merges</small></>;
  if (model.source === "trust") return <Planned why={WHY.cost} />;
  const costs = costsOf(model);
  return costs.length === 0 ? <small>No cost reported</small> : <>{costs.map((cost) => <small key={`${cost.engine}-${cost.unit}`}>{cost.engine} {costText(cost)}</small>)}</>;
}

function Row({ read, name, onOpen }: { read: RepoRead; name: string; onOpen(id: string): void }) {
  if ("failure" in read) {
    return (
      <>
        <div><b>{name}</b></div>
        <div style={{ gridColumn: "2 / span 4" }}><b>Couldn’t read</b><small>{failureLine(read.failure)}</small></div>
        <div><button type="button" className="cx-ghost" aria-label={`Open ${name}`} onClick={() => onOpen(read.project)}>Open</button></div>
      </>
    );
  }
  const { model } = read;
  return (
    <>
      <div><b>{name}</b>{model.engines.length > 0 ? <small>{enginesLine(model)}</small> : null}</div>
      <div><Autonomy model={model} /></div>
      <div><Cycle model={model} /></div>
      <div><Reverts model={model} /></div>
      <div><Cost model={model} /></div>
      <div><button type="button" className="cx-ghost" aria-label={`Open ${name}`} onClick={() => onOpen(read.project)}>Open</button></div>
    </>
  );
}

const failureLine = (failure: ApiFailure) => text(failure.message, 120) || "The history could not be read.";

/**
 * All projects: one row per repository with autonomy, median cycle time, revert rate and cost, each with its sample, and Open for that repository's own
 * page. The totals below add up one engine in one unit at a time; two engines' units are never joined into one figure.
 */
export function AllRepos({ client, projects, days, onOpen }: { client: DashboardClient; projects: readonly Choosable[]; days: Period; onOpen(id: string): void }) {
  const read = useEveryRepo(client, projects.map((entry) => entry.id), days);
  const slow = useWaited(read.status === "loading");
  if (projects.length === 0) return <small className="cx-t10f">There is no project to show.</small>;
  if (read.status === "loading") return slow ? <Loading label="Reading every project" rows={4} /> : <span className="sr-only" role="status">Reading every project</span>;
  if (read.status === "error") return <Failure failure={read.error} onRetry={() => void read.reload()} />;
  const names = new Map(projects.map((entry) => [entry.id, text(entry.name) || "Unnamed project"]));
  const models = read.data.flatMap((one) => ("model" in one ? [one.model] : []));
  const totals = costTotals(models);
  const minRate = models[0]?.minRate ?? 10;
  return (
    <>
      <div className="cx-aptb" style={{ gridTemplateColumns: GRID }} data-testid="all-projects">
        {["Repo", "Autonomy", "Cycle time", "Revert rate", "Cost, per engine in its own unit", ""].map((name, at) => <div key={`${name}-${at}`} className="cx-h">{name}</div>)}
        {read.data.map((one) => <Row key={one.project} read={one} name={names.get(one.project) ?? "Unnamed project"} onOpen={onOpen} />)}
      </div>
      <div className="cx-fbx">
        <h6>All projects, cost per engine</h6>
        {models.some((model) => model.source === "history") ? (
          <p>
            {totals.length > 0 ? `${totals.map((cost) => `${cost.engine} ${costText(cost)}`).join(" · ")}. ` : "No engine reported a cost. "}
            Each total adds up that one engine across the repos above, in its own unit. Each engine stays in its own row and is never added into one figure. Repos with fewer than {minRate} merges show the merges, not a rate.
          </p>
        ) : (
          <p>
            Cost per engine <Planned why={WHY.cost} /> Repos with fewer than {minRate} merges show the merges, not a rate.
          </p>
        )}
        {projects.length > MAX_REPOS ? <p>The first {MAX_REPOS} of {projects.length} projects are shown.</p> : null}
      </div>
    </>
  );
}
