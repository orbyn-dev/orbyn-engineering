import type { Project } from "../../../../api/types";
import { NETWORK_ERROR, type ApiFailure } from "../../../../api/client";
import { Loading } from "../../../../product/ui/states";
import type { EscapeLog } from "../../../../generated/EscapeLog";
import { Frame } from "../../shared/Frame";
import { Btn, Ic } from "../../shared/parts";
import { Planned } from "../../capacity/Planned";
import { asOf, count, ESCAPES_LIMIT, merges, WHY } from "./model";
import { EscapeTable, ProjectChip } from "./parts";
import { useEscapeLog } from "./useEscapes";

/** The dash every tile shows while the log can't be read: never a zero, because nothing was measured. */
function DashTiles() {
  return (
    <div className="cx-t10g">
      {["Escapes", "Why it got through", "Checks from escapes", "Counted against"].map((label) => (
        <div className="cx-t10c" key={label}>
          <span className="cx-t10l">{label}</span>
          <span className="cx-t10b">—</span>
          <span className="cx-t10f">Not shown while the log can’t be read</span>
        </div>
      ))}
    </div>
  );
}

function LogError({ failure, onRetry }: { failure: ApiFailure; onRetry: () => void }) {
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
        <span>Can’t read escapes. The escape log on this device can’t be read, so nothing is shown. Nothing was changed, and no escape was counted or removed.</span>
        <span className="cx-acts">
          <button type="button" className="cx-ghost" onClick={copy}>Copy the error</button>
          <button type="button" className="cx-btn" onClick={onRetry}>Try again</button>
        </span>
      </div>
      <small style={{ color: "var(--color-muted)" }}>{detail}</small>
      <DashTiles />
    </>
  );
}

/** One engine's counts, never added together, with its own low-sample flag. */
function EngineCounts({ log }: { log: EscapeLog }) {
  return (
    <>
      <div className="cx-t10e">
        {log.engines.map((row) => (
          <span key={row.engine}>
            <i>{row.engine}</i>
            <b>
              {count(row.confirmed)} · {count(row.possible)} · {count(row.dismissed)} of {count(row.merges)}
              {row.low_sample ? " · low sample" : ""}
            </b>
          </span>
        ))}
      </div>
      <span className="cx-t10f">Last {log.window_days} days · a rate needs {log.min_merges} merges</span>
    </>
  );
}

/** The four tiles: two of them carry no number until the report is built, and say so. */
function Tiles({ log }: { log: EscapeLog }) {
  return (
    <div className="cx-t10g">
      <div className="cx-t10c">
        <span className="cx-t10l">Escapes</span>
        <span className="cx-t10b">
          {count(log.confirmed)}
          <small>confirmed</small>
        </span>
        <span className="cx-t10s">{count(log.possible)} possible · {count(log.dismissed)} dismissed, counted apart and never added</span>
        <span className="cx-t10f">
          of {count(log.merges)} merges · last {log.window_days} days{log.low_sample ? " · low sample" : ""} · Rate: Not measured{" "}
          <Planned why={WHY.report} />
        </span>
      </div>
      <div className="cx-t10c">
        <span className="cx-t10l">Why it got through</span>
        <span className="cx-t10b">Not measured</span>
        <span className="cx-t10s">
          test gaps and tooling gaps <Planned why={WHY.report} />
        </span>
        <span className="cx-t10f">The counts come with the report. Each escape’s own gap is in the table.</span>
      </div>
      <div className="cx-t10c">
        <span className="cx-t10l">Checks from escapes</span>
        <span className="cx-t10b">Not measured</span>
        <span className="cx-t10s">
          checks added, being written and failed <Planned why={WHY.report} />
        </span>
        <span className="cx-t10f">The counts come with the report. A check’s state is on the escape.</span>
      </div>
      <div className="cx-t10c">
        <span className="cx-t10l">Counted against</span>
        <span className="cx-t10b">Per engine</span>
        <span className="cx-t10s">confirmed · possible · dismissed, of the engine’s merges</span>
        <EngineCounts log={log} />
      </div>
    </div>
  );
}

/**
 * Crew Designs v79, `escape-log` with its empty, low-sample and unreadable states, and `escape-limit` (the banner the
 * writer's own refusal draws when a project holds its 5,000 escapes), on the real escape log read. The limit is the core's
 * own cap and the read carries the count, so the page says it is reached only when the count does. Counts are shown apart
 * and never added; there is no rate in this slice, so the tiles say so instead of inventing one. The window is the stored
 * one, so the header names it and offers no window of its own. Nothing here sends a write: the person's acts are on the
 * possible list and on the escape's own page.
 */
export function LiveEscapeLog({ project, name, projects, onPick }: { project: string; name: string; projects: readonly Project[]; onPick(id: string): void }) {
  const read = useEscapeLog(project);
  const log = read.status === "ready" ? read.data : null;
  const open = (id: string) => {
    window.location.hash = `#/preview/crew/escape-detail?id=${encodeURIComponent(id)}`;
  };
  return (
    <Frame live>
      <div className="cx-cwpg">
        <div className="cx-flcr">
          <span>Dashboard</span>
          <Ic n="right" sm />
          <b>Escape log</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Escape log</b>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <ProjectChip name={name} projects={projects} current={project} onPick={onPick} />
              {log ? <span className="cx-cwseg"><span className="cx-on">Last {log.window_days} days</span></span> : null}
            </span>
          </div>
          {read.status === "loading" ? <Loading label="Reading escapes" rows={4} /> : null}
          {read.status === "error" ? <LogError failure={read.error} onRetry={() => void read.reload()} /> : null}
          {log ? (
            <>
              <span className="cx-t10f" style={{ margin: "0", color: "var(--color-muted)", fontSize: "11.5px" }}>As of {asOf(log.as_of)}</span>
              {log.low_sample ? (
                <div className="cx-cwbn">
                  <Ic n="info" sm />
                  <span>
                    Fewer than {count(log.min_merges)} merges in the last {log.window_days} days ({count(log.merges)}). The log counts what is there; a rate would mostly say luck, and none is shown in v1 anyway <Planned why={WHY.report} />.
                  </span>
                </div>
              ) : null}
              {log.total >= ESCAPES_LIMIT ? (
                <div className="cx-cwbn cx-amber" role="status">
                  <Ic n="alert" sm />
                  <span>
                    This project has reached the {count(ESCAPES_LIMIT)}-escape limit. A new escape is not recorded, and Mark as an
                    escape is refused, until room is made. The same holds at 20,000 audit events. Confirmed escapes are never dropped.
                  </span>
                  <span className="cx-acts">
                    <Planned why={WHY.pruning} />
                  </span>
                </div>
              ) : null}
              {log.total === 0 ? (
                <div className="cx-cwstate" style={{ margin: "48px auto 0" }}>
                  <b>No escapes in the last {log.window_days} days</b>
                  <p>
                    {merges(log.merges)}, and no possible, confirmed or dismissed escape. A revert appears here at once as a
                    possible escape; so can a later fix or a failed check Crew notices. A merge you mark is confirmed at once.
                  </p>
                  <span style={{ display: "flex", alignItems: "center", gap: "6px" }}>
                    <Planned why={WHY.queue} style={{ marginLeft: "0" }} />
                    <Btn className="cx-ghost" style={{ height: "28px" }} reason={`Planned: ${WHY.queue}`}>Open the Review queue</Btn>
                  </span>
                </div>
              ) : (
                <>
                  <Tiles log={log} />
                  <h5 className="cx-t10h5">
                    The escapes <span className="cx-t10r">{count(log.total)} in all</span>
                    {log.total > log.rows.length ? <span className="cx-t10r"> · Showing {count(log.rows.length)} of {count(log.total)}</span> : null}
                  </h5>
                  <EscapeTable rows={log.rows} onOpen={open} />
                </>
              )}
              <div className="cx-fbx">
                <h6>What counts as an escape</h6>
                <div className="cx-fkv">
                  <span>A possible escape</span>
                  <b>
                    Found by Crew and never confirmed by itself: a revert (at once), a later fix on a line the merge added, or a
                    repository check that failed on this device after the merge. A forge’s CI status is not read{" "}
                    <Planned why={WHY.forge} />. It is not counted until you confirm it
                  </b>
                  <span>A confirmed escape</span>
                  <b>
                    You confirmed a possible escape, or you marked the merge. It is final: there is no undo in v1{" "}
                    <Planned why={WHY.undo} />
                  </b>
                  <span>Dismissed</span>
                  <b>You said it is not an escape. It is not counted and not offered again. It is final too</b>
                  <span>A rate</span>
                  <b>
                    None in v1: the log counts. A rate comes with the report <Planned why={WHY.report} />, from{" "}
                    {log.min_merges} merges, and counts confirmed escapes only; possible ones stay apart
                  </b>
                  <span>What it feeds</span>
                  <b>Nothing yet: confirmed escapes feed neither the circuit breaker nor the Router</b>
                </div>
              </div>
            </>
          ) : null}
        </div>
      </div>
    </Frame>
  );
}
