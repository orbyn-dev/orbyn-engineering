import { routeHref } from "../../../product/state/route";
import { Planned, WHY } from "./parts";
import { costText, mergesOf, rate, type AreaRow, type DashModel, type EngineRow } from "./model";

const GRID_ENGINE = "minmax(0,1.3fr) minmax(0,1.3fr) minmax(0,1fr) minmax(0,1.1fr)";
const GRID_AREA = "minmax(0,1.3fr) minmax(0,1.6fr) minmax(0,1fr)";

function OpenTrust() {
  return (
    <a className="cx-t10r" href={routeHref({ name: "crew", tab: "trust" })} style={{ color: "inherit" }}>
      Open Trust
    </a>
  );
}

/** "76% merged without you · 16 of 21", or "3 of 5 merged without you · too few for a rate"; `null` when nothing records it. */
function autonomyCell(row: { merges: number; unaided: number | null }, minRate: number) {
  if (row.unaided === null) return <><b>{mergesOf(row.merges)}</b><small>Autonomy is Planned</small></>;
  const r = rate(row.unaided, row.merges, minRate);
  return <><b>{mergesOf(row.merges)}</b><small>{r.pct ? `${r.pct} merged without you · ${r.of}` : `${r.of} merged without you · ${r.few}`}</small></>;
}

function revertCell(row: { merges: number; reverted: number }, minRate: number) {
  const r = rate(row.reverted, row.merges, minRate);
  return <><b>{r.of}</b><small>{r.pct ? `reverted · ${r.pct}` : r.few}</small></>;
}

function EngineLine({ row, minRate, source }: { row: EngineRow; minRate: number; source: DashModel["source"] }) {
  return (
    <>
      <div><b>{row.model ? `${row.name} · ${row.model}` : row.name}</b></div>
      <div>{autonomyCell(row, minRate)}</div>
      <div>{revertCell(row, minRate)}</div>
      <div>
        {row.cost ? <><b>{costText(row.cost)}</b><small>{row.name}’s own unit</small></> : source === "trust" ? <Planned why={WHY.cost} /> : <small>No cost reported</small>}
      </div>
    </>
  );
}

/** Merges, autonomy, reverts and cost by engine. Cost is in each engine's own unit and is never added into one figure. */
export function EngineTable({ model }: { model: DashModel }) {
  if (model.engines.length === 0) return null;
  return (
    <>
      <h5 className="cx-t10h5">By engine<OpenTrust /></h5>
      <div className="cx-aptb" style={{ gridTemplateColumns: GRID_ENGINE }}>
        {["Engine", "Autonomy", "Reverts", "Cost"].map((name) => <div key={name} className="cx-h">{name}</div>)}
        {model.engines.map((row, at) => <EngineLine key={`${row.name}-${at}`} row={row} minRate={model.minRate} source={model.source} />)}
      </div>
    </>
  );
}

function AreaLine({ row, minRate }: { row: AreaRow; minRate: number }) {
  return (
    <>
      <div><b>{row.name}</b></div>
      <div>{autonomyCell(row, minRate)}</div>
      <div>{revertCell(row, minRate)}</div>
    </>
  );
}

/** Merges and reverts by area. Cost is not split by area: one area's work can use engines counted in different units. */
export function AreaTable({ model }: { model: DashModel }) {
  if (model.areas.length === 0) return null;
  return (
    <>
      <h5 className="cx-t10h5">By area<OpenTrust /></h5>
      <div className="cx-aptb" style={{ gridTemplateColumns: GRID_AREA }}>
        {["Area", "Autonomy", "Reverts"].map((name) => <div key={name} className="cx-h">{name}</div>)}
        {model.areas.map((row, at) => <AreaLine key={`${row.name}-${at}`} row={row} minRate={model.minRate} />)}
      </div>
      {model.truncated ? <small className="cx-t10f">More rows exist than are listed.</small> : null}
    </>
  );
}
