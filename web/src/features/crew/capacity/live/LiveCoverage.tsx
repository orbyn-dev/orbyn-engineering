import type { ReviewCoverage } from "../../../../generated/ReviewCoverage";
import { Loading } from "../../../../product/ui/states";
import { NETWORK_ERROR, type ApiFailure } from "../../../../api/client";
import { Frame } from "../../shared/Frame";
import { Ic, Seg } from "../../shared/parts";
import { count, dateOf, engineLabel, merges, momentOf, NOT_MEASURED, share, split } from "./model";
import { useCoverage } from "./useCapacity";

const TABLE = "minmax(0,1.1fr) minmax(0,1.2fr) minmax(0,.9fr) minmax(0,.9fr) minmax(0,1fr) minmax(0,1.1fr)";
const ENGINE_TABLE = "minmax(0,1.3fr) minmax(0,1.2fr) minmax(0,1fr) minmax(0,1.1fr)";

/**
 * Crew Designs v21, `capacity-coverage` and its empty, gaps and error states, on the real coverage read. Coverage is counted
 * at merge level: a line is reviewed when the merge that wrote it was marked Looks good or commented, never per line. Anything
 * the read could not measure says Not measured and adds no number. Nothing here sends a write.
 */
export function LiveCoverage({ project, name }: { project: string; name: string }) {
  const read = useCoverage(project);
  return (
    <Frame live>
      <div className="cx-cwpg">
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Review coverage</b>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />{name}</span>
              <Seg kind="cwseg" on={0} items={["Since provenance began"]} />
            </span>
          </div>
          {read.status === "loading" ? <Loading label="Reading review coverage" rows={3} /> : null}
          {read.status === "error" ? <CoverageError failure={read.error} onRetry={() => void read.reload()} /> : null}
          {read.status === "ready" ? <CoverageMap coverage={read.data} name={name} /> : null}
        </div>
      </div>
    </Frame>
  );
}

function CoverageError({ failure, onRetry }: { failure: ApiFailure; onRetry: () => void }) {
  const detail = failure.code === NETWORK_ERROR ? failure.message : `${failure.code}: ${failure.message}`;
  const copy = () => {
    try {
      void navigator.clipboard?.writeText(detail).catch(() => undefined);
    } catch {
      /* no clipboard here: the text stays visible below */
    }
  };
  return (
    <>
      <div className="cx-cwbn cx-red" style={{ margin: "0" }} role="alert">
        <Ic n="alert" sm />
        <span>Couldn’t read the provenance notes on this device. Nothing was changed, and no line was counted or removed.</span>
        <span className="cx-acts">
          <button type="button" className="cx-ghost" onClick={copy}>Copy the error</button>
          <button type="button" className="cx-btn" onClick={onRetry}>Try again</button>
        </span>
      </div>
      <small style={{ color: "var(--color-muted)" }}>{detail}</small>
      <div className="cx-t15tile"><b className="cx-n">—</b><small>Not shown while the notes can’t be read</small></div>
    </>
  );
}

function CoverageMap({ coverage, name }: { coverage: ReviewCoverage; name: string }) {
  const since = coverage.began_unix;
  // Recording never began, or no merge has lines: there is no map, and nothing is counted as zero.
  if (since == null || coverage.agent_lines === 0) {
    return (
      <div className="cx-cwstate" style={{ margin: "48px auto 0" }}>
        <b>No agent-written lines are recorded yet</b>
        <p>
          {since == null
            ? `Crew records which lines an agent wrote when it merges. Recording has not begun for ${name} on this device, so every merge is ${NOT_MEASURED}.`
            : `Crew records which lines an agent wrote when it merges. No merge in ${name} since recording began has lines to count, so there is nothing to show.`}
        </p>
        {coverage.not_measured_merges > 0 ? <p>{`${merges(coverage.not_measured_merges)} ${coverage.not_measured_merges === 1 ? "is" : "are"} ${NOT_MEASURED}.`}</p> : null}
      </div>
    );
  }
  const unread = Math.max(0, coverage.agent_lines - coverage.reviewed_lines);
  const total = coverage.merges + coverage.not_measured_merges;
  return (
    <>
      <span className="cx-t10f" style={{ margin: "0", color: "var(--color-muted)", fontSize: "11.5px" }}>
        {`As of ${momentOf(coverage.as_of_unix)} · ${count(coverage.agent_lines)} agent-written lines from ${merges(coverage.merges)} since ${dateOf(since)}`}
      </span>
      {coverage.not_measured_merges > 0 ? (
        <div className="cx-cwbn" style={{ margin: "0" }}>
          <Ic n="info" sm />
          <span>
            <b>{`${coverage.not_measured_merges} of ${total} merges aren’t in the map.`}</b>
            {` They are from before ${dateOf(since)}, had no stored hunks or had no note written. Their lines are ${NOT_MEASURED}, not counted as read or unread.`}
          </span>
        </div>
      ) : null}
      <div className="cx-t15map">
        {coverage.areas.map((area) => {
          const { unread: left, notInQueue } = split(area);
          const pct = share(left, area.agent_lines);
          const none = area.agent_lines === 0;
          return (
            <div className="cx-t15tile" key={area.area}>
              <b className="cx-a">{area.area}</b>
              <b className="cx-n">{pct ? `${count(left)} of ${count(area.agent_lines)}` : NOT_MEASURED}</b>
              <small>{pct ? `lines no person has reviewed · ${pct}` : `No lines are recorded for this area · ${merges(area.not_measured_merges)} ${NOT_MEASURED}`}</small>
              <div className="cx-sp3">
                <span><i>Reviewed</i>{none ? NOT_MEASURED : count(area.reviewed_lines)}</span>
                <span><i>In your queue</i>{none ? NOT_MEASURED : count(area.in_queue_lines)}</span>
                <span><i>Not in a queue</i>{none ? NOT_MEASURED : count(notInQueue)}</span>
              </div>
              <small>{none ? `0 merges with lines` : `${merges(area.merges)}${area.not_measured_merges > 0 ? ` · ${merges(area.not_measured_merges)} ${NOT_MEASURED}` : ""}`}</small>
            </div>
          );
        })}
      </div>
      <h5 className="cx-t10h5">By area</h5>
      <div className="cx-aptb" style={{ gridTemplateColumns: TABLE }}>
        <div className="cx-h">Area</div>
        <div className="cx-h">Agent-written lines</div>
        <div className="cx-h">Reviewed</div>
        <div className="cx-h">In your queue</div>
        <div className="cx-h">Not in a queue</div>
        <div className="cx-h">No person has read</div>
        {coverage.areas.flatMap((area) => {
          const { unread: left, notInQueue } = split(area);
          const none = area.agent_lines === 0;
          const cell = (n: number) => (none ? NOT_MEASURED : count(n));
          return [
            <div key={`${area.area}-0`}><b>{area.area}</b><small>{none ? `${merges(area.not_measured_merges)} ${NOT_MEASURED}` : `${merges(area.merges)}${area.not_measured_merges > 0 ? ` · ${merges(area.not_measured_merges)} ${NOT_MEASURED}` : ""}`}</small></div>,
            <div key={`${area.area}-1`}><b>{cell(area.agent_lines)}</b></div>,
            <div key={`${area.area}-2`}><b>{cell(area.reviewed_lines)}</b></div>,
            <div key={`${area.area}-3`}><b>{cell(area.in_queue_lines)}</b></div>,
            <div key={`${area.area}-4`}><b>{cell(notInQueue)}</b></div>,
            <div key={`${area.area}-5`}><b>{cell(left)}</b>{none ? null : <small>{share(left, area.agent_lines)}</small>}</div>,
          ];
        })}
        <div><b>All areas</b><small>{merges(coverage.merges)}</small></div>
        <div><b>{count(coverage.agent_lines)}</b></div>
        <div><b>{count(coverage.reviewed_lines)}</b></div>
        <div><b>{count(coverage.in_queue_lines)}</b></div>
        <div><b>{count(split(coverage).notInQueue)}</b></div>
        <div><b>{count(unread)}</b><small>{share(unread, coverage.agent_lines)}</small></div>
      </div>
      <h5 className="cx-t10h5">By engine</h5>
      <div className="cx-aptb" style={{ gridTemplateColumns: ENGINE_TABLE }}>
        <div className="cx-h">Engine</div>
        <div className="cx-h">Agent-written lines</div>
        <div className="cx-h">Reviewed</div>
        <div className="cx-h">No person has read</div>
        {coverage.engines.flatMap((engine) => {
          const left = Math.max(0, engine.agent_lines - engine.reviewed_lines);
          return [
            <div key={`${engine.engine}-${engine.model ?? ""}-0`}><b>{engineLabel(engine)}</b></div>,
            <div key={`${engine.engine}-${engine.model ?? ""}-1`}><b>{count(engine.agent_lines)}</b></div>,
            <div key={`${engine.engine}-${engine.model ?? ""}-2`}><b>{count(engine.reviewed_lines)}</b></div>,
            <div key={`${engine.engine}-${engine.model ?? ""}-3`}><b>{count(left)}</b><small>{share(left, engine.agent_lines) ?? NOT_MEASURED}</small></div>,
          ];
        })}
      </div>
      <div className="cx-fbx">
        <h6>How to read this</h6>
        <div className="cx-fkv">
          <span>Agent-written</span>
          <b>Lines the merges wrote, counted per merge from the note Crew records at merge. Not the lines still on main today</b>
          <span>Reviewed</span>
          <b>The merge that wrote the lines was marked Looks good or commented. Review is counted per merge, never per line</b>
          <span>In your queue</span>
          <b>The change that wrote it is waiting for your review</b>
          <span>Not in a queue</span>
          <b>Lines of a change that is neither reviewed nor waiting in your queue</b>
          <span>Shares</span>
          <b>{`Exact counts of lines. They cover only merges since ${dateOf(since)}${coverage.not_measured_merges > 0 ? `: ${merges(coverage.not_measured_merges)} ${coverage.not_measured_merges === 1 ? "is" : "are"} ${NOT_MEASURED}` : ""}`}</b>
        </div>
      </div>
    </>
  );
}
