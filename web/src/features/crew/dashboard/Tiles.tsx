import { Bars, Tile, WHY } from "./parts";
import { costsOf, costText, duration, mergesOf, medianOf, rate, type DashModel } from "./model";

export type Metric = "autonomy" | "revert";

const windowOf = (model: DashModel) => (model.days > 0 ? `last ${model.days} days` : "in the window");

function OpenByWeek({ metric, onOpen }: { metric: Metric; onOpen(metric: Metric): void }) {
  return (
    <button type="button" className="cx-t10r" style={{ alignSelf: "flex-start", textDecoration: "underline", textUnderlineOffset: "2px", color: "var(--color-textSoft)", fontSize: "11.5px" }} onClick={() => onOpen(metric)}>
      {metric === "autonomy" ? "Open autonomy by week" : "Open revert rate by week"}
    </button>
  );
}

function AutonomyTile({ model, onOpen, compact }: { model: DashModel; onOpen(metric: Metric): void; compact: boolean }) {
  if (model.unaided === null) {
    return <Tile label="Autonomy" planned={WHY.autonomy} value="—" sub="merged without your action" sample="Planned: nothing records what a merge needed from you yet" />;
  }
  const r = rate(model.unaided, model.merges, model.minRate);
  return (
    <Tile
      label="Autonomy"
      value={r.pct ?? r.of}
      sub="merged without your action"
      sample={r.pct ? `${r.of} merges · ${windowOf(model)}${compact ? "" : ". Bars: merges per week, filled = needed nothing from you"}` : `${mergesOf(model.merges)} · ${r.few}`}
    >
      {!compact && model.weeks && model.weeks.length > 0 ? <><Bars weeks={model.weeks} mode="unaided" /><OpenByWeek metric="autonomy" onOpen={onOpen} /></> : null}
    </Tile>
  );
}

function CycleTile({ model }: { model: DashModel }) {
  if (model.source === "trust") return <Tile label="Cycle time" planned={WHY.cycle} value="—" sub="from brief to merged" sample="Planned: nothing records when a brief was given" />;
  const median = medianOf(model);
  if (median === null) {
    return <Tile label="Cycle time" value={<>Not measured</>} sub="from brief to merged" sample={`${mergesOf(model.merges)} · a median needs at least ${model.minMedian}`} />;
  }
  const cycle = model.cycle;
  return (
    <Tile label="Cycle time" value={<>{duration(median)}<small>median</small></>} sub="from brief to merged" sample={`${mergesOf(model.merges)} · ${model.merges < model.minRate ? `a median needs at least ${model.minMedian}` : windowOf(model)}`}>
      <div className="cx-t10e">
        {cycle?.p90 != null ? <span><i>90th percentile</i><b>{duration(cycle.p90)}</b></span> : null}
        {cycle?.fastest != null ? <span><i>Fastest</i><b>{duration(cycle.fastest)}</b></span> : null}
        {cycle?.slowest != null ? <span><i>Slowest</i><b>{duration(cycle.slowest)}</b></span> : null}
      </div>
    </Tile>
  );
}

function CostTile({ model }: { model: DashModel }) {
  if (model.source === "trust") return <Tile label="Cost" planned={WHY.cost} value="Per engine" sub="each in its own unit, never added up" sample="Planned: nothing totals what each engine used yet" />;
  const costs = costsOf(model);
  return (
    <Tile label="Cost" value="Per engine" sub="each in its own unit, never added up" sample={`${mergesOf(model.merges)} · ${windowOf(model)}`}>
      <div className="cx-t10e">
        {costs.length === 0 ? <span><i>No engine reported a cost</i></span> : costs.map((cost) => <span key={`${cost.engine}-${cost.unit}`}><i>{cost.engine}</i><b>{costText(cost)}</b></span>)}
      </div>
    </Tile>
  );
}

function RevertTile({ model, onOpen, compact }: { model: DashModel; onOpen(metric: Metric): void; compact: boolean }) {
  const r = rate(model.reverted, model.merges, model.minRate);
  const split = model.revertedAuto !== null && model.revertedByPerson !== null && model.reverted > 0 ? `${model.revertedAuto} reverted automatically, ${model.revertedByPerson} by you · ` : "";
  // The trust record names its own window and nothing else on the page does, so the tile says it.
  const span = model.source === "trust" ? ` · ${windowOf(model)}` : "";
  return (
    <Tile
      label="Revert rate"
      value={r.pct ?? r.of}
      sub={r.pct ? `${r.of} merges reverted` : "reverted"}
      sample={r.pct ? `${split}${mergesOf(model.merges)}${span}${compact ? "" : ". Bars: merges per week, filled = reverted"}` : `${split}${mergesOf(model.merges)}${span} · ${r.few}`}
    >
      {!compact && model.weeks && model.weeks.length > 0 ? <><Bars weeks={model.weeks} mode="reverted" /><OpenByWeek metric="revert" onOpen={onOpen} /></> : null}
    </Tile>
  );
}

/** The four tiles in the order the design draws them. Each says what it rests on; a figure the read cannot give is a Planned tile, never a zero. */
export function Tiles({ model, onOpen, compact }: { model: DashModel; onOpen(metric: Metric): void; compact: boolean }) {
  return (
    <div className="cx-t10g" data-testid="dashboard-tiles">
      <AutonomyTile model={model} onOpen={onOpen} compact={compact} />
      <CycleTile model={model} />
      <CostTile model={model} />
      <RevertTile model={model} onOpen={onOpen} compact={compact} />
    </div>
  );
}
