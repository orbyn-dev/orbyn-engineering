import { useEffect, useRef, useState } from "react";
import type { FlowCounts } from "../../../../generated/FlowCounts";
import type { FlowFilter } from "../../../../generated/FlowFilter";
import type { FlowTaskRow } from "../../../../generated/FlowTaskRow";
import { ErrorState, Empty, Loading } from "../../../../product/ui/states";
import { Button } from "../../../../ui/controls";
import { useCrewPreview } from "../../autopilot/approvals/useCrew";
import { CellGlyph, SearchIcon } from "./glyphs";
import { cellOf, cellView, countOf, FILTERS, roundView, shown, STAGES } from "./model";
import { FlowZoom } from "./FlowZoom";
import { useFlowCrews, useFlowTasks, useRefresh } from "./useFlow";
import "./flow-live.css";

const AFTER_THE_BETA = "After the beta: no route yet. The beta has a Cancel on each task, when it is wired.";

/**
 * The Flow tab of the Crew view on the real Flow routes. Nothing is drawn while Crew Preview is off (a service with it off
 * answers 404, and so does one with no Crew routes: both read as off, and no other route is read). Reads only: nothing here
 * cancels, re-runs, approves or merges. The service's own text is drawn as plain text.
 */
export function FlowView() {
  return useCrewPreview() ? <Flow /> : null;
}

function Flow() {
  const crews = useFlowCrews();
  const [picked, setPicked] = useState<string | null>(null);
  const [task, setTask] = useState<string | null>(null);
  useRefresh(crews.reload, crews.status !== "ready");

  if (crews.status === "loading") return <Loading label="Reading Flow" rows={4} />;
  if (crews.status === "error" && !crews.data) return <ErrorState title="Can’t read Flow" failure={crews.error} onRetry={() => void crews.reload()} />;
  const list = crews.data ?? [];
  if (list.length === 0) return <Empty title="No crews running" />;
  const crew = list.find((one) => one.id === picked) ?? list[0]!;

  return (
    <section className="fv-root" aria-label="Flow">
      <div className="fv-crew">
        <label htmlFor="fv-crew-pick">Crew</label>
        <select
          id="fv-crew-pick"
          value={crew.id}
          onChange={(event) => {
            setPicked(event.target.value);
            setTask(null);
          }}
        >
          {list.map((one) => (
            <option key={one.id} value={one.id}>
              {one.label} · {one.tasks} {one.tasks === 1 ? "task" : "tasks"}, {one.finished} finished
            </option>
          ))}
        </select>
        <span className="fv-spacer" />
        {/* No Flow route pauses or stops a crew: the design draws both After the beta. */}
        {["Pause crew", "Stop crew"].map((name) => (
          <button key={name} type="button" className="fv-off" disabled aria-description={AFTER_THE_BETA} title={AFTER_THE_BETA}>
            {name}
            <span className="fv-label">After the beta</span>
          </button>
        ))}
      </div>
      {task ? <FlowZoom key={task} id={task} onBack={() => setTask(null)} /> : <CrewTasks key={crew.id} crew={crew.id} onOpen={setTask} />}
    </section>
  );
}

function CrewTasks({ crew, onOpen }: { crew: string; onOpen(id: string): void }) {
  const [filter, setFilter] = useState<FlowFilter>("all");
  const [draft, setDraft] = useState("");
  const [q, setQ] = useState("");
  const [engine, setEngine] = useState<string | null>(null);
  const [engines, setEngines] = useState<string[]>([]);
  const counts = useRef<FlowCounts | null>(null);
  useEffect(() => {
    const timer = setTimeout(() => setQ(draft), 250);
    return () => clearTimeout(timer);
  }, [draft]);

  const { first, rows, next, showMore, busy, failure } = useFlowTasks({ crew, filter, q, engine });
  useRefresh(first.reload, first.status !== "ready" || rows.length > first.data!.tasks.length);
  const page = first.data;
  if (page) counts.current = page.counts;
  // The engines to pick from are the ones seen on any list read for this crew, so choosing one does not empty the choice.
  const seen = page ? page.tasks.flatMap((row) => (row.engine ? [row.engine] : [])) : [];
  useEffect(() => {
    if (seen.length === 0) return;
    setEngines((now) => {
      const merged = [...new Set([...now, ...seen])].sort();
      return merged.length === now.length ? now : merged;
    });
    // `seen` is rebuilt from `page` each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [page]);

  const chips = counts.current;
  return (
    <div className="fv-body">
      <div className="fv-tool" role="search">
        <label className="fv-search">
          <SearchIcon />
          <span className="sr-only">Find a task</span>
          <input type="search" placeholder="Find a task" value={draft} maxLength={80} onChange={(event) => setDraft(event.target.value)} />
        </label>
        <div className="fv-chips" role="group" aria-label="Filter tasks">
          {FILTERS.map((one) => (
            <button key={one.filter} type="button" className="fv-chip" aria-pressed={filter === one.filter} onClick={() => setFilter(one.filter)}>
              {one.label}
              {chips ? ` ${countOf(chips, one.filter)}` : ""}
            </button>
          ))}
        </div>
        <label className="fv-engine">
          <span className="sr-only">Engine</span>
          <select value={engine ?? ""} onChange={(event) => setEngine(event.target.value === "" ? null : event.target.value)}>
            <option value="">Engine: any</option>
            {[...new Set([...engines, ...(engine ? [engine] : [])])].map((one) => (
              <option key={one} value={one}>
                {one}
              </option>
            ))}
          </select>
        </label>
        {page ? <span className="fv-count">{shown(rows.length, page.total)}</span> : null}
      </div>
      {first.status === "loading" ? <Loading label="Reading tasks" rows={4} /> : null}
      {first.status === "error" && !page ? <ErrorState title="Can’t read Flow" failure={first.error} onRetry={() => void first.reload()} /> : null}
      {page ? (
        <>
          {page.truncated ? (
            <p className="fv-note" role="note">
              This crew has more tasks than one read covers; the newest are shown.
            </p>
          ) : null}
          {rows.length === 0 ? <Empty title={filter === "all" && q.trim() === "" && !engine ? "No tasks yet" : "No tasks match"} /> : <TaskTable rows={rows} onOpen={onOpen} />}
          {failure ? <ErrorState compact title="Can’t read more tasks" failure={failure} onRetry={() => void showMore()} /> : null}
          {next !== null ? (
            <div className="fv-more">
              <Button small busy={busy} onClick={() => void showMore()}>
                +{Math.max(0, page.total - rows.length)} more
              </Button>
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}

function TaskTable({ rows, onOpen }: { rows: FlowTaskRow[]; onOpen(id: string): void }) {
  return (
    <div className="fv-scroll">
      <div className="fv-table" role="list" aria-label="Tasks">
        <div className="fv-r fv-h" aria-hidden>
          <span>Task</span>
          {STAGES.map((one) => (
            <span key={one.stage} className="fv-col">
              {one.column}
            </span>
          ))}
          <span className="fv-col">Round</span>
        </div>
        {rows.map((row) => {
          const round = roundView(row);
          return (
            <div key={row.id} role="listitem">
              <button type="button" className="fv-r fv-row" onClick={() => onOpen(row.id)}>
                <span className="fv-name">
                  <b>{row.name || "Untitled task"}</b>
                  <small>
                    {[row.kind, row.engine, row.owner ? `owner ${row.owner}` : ""].filter(Boolean).join(" · ")}
                  </small>
                </span>
                {STAGES.map((one) => {
                  const view = cellView(cellOf(row, one.stage), one.name);
                  return (
                    <span key={one.stage} className={`fv-fc fv-${view.tone}`} title={view.label}>
                      <span className="sr-only">{view.label}</span>
                      <CellGlyph glyph={view.glyph} />
                      {view.text ? <span aria-hidden>{view.text}</span> : null}
                    </span>
                  );
                })}
                <span className={round.tone === "plain" ? "fv-fc" : `fv-fc fv-${round.tone}`}>{round.text}</span>
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
