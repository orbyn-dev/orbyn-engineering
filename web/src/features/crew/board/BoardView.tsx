import { useState } from "react";
import type { FlowCard } from "../../../generated/FlowCard";
import type { FlowColumnId } from "../../../generated/FlowColumnId";
import { ErrorState, Empty, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { useFlowCrews } from "../flow/live/useFlow";
import { useTaskWrite, WriteStatus } from "../writes/TaskWrite";
import { CardMenu } from "./CardMenu";
import { ago, COLUMNS, modelLine, moreOf, type ColumnSpec } from "./model";
import { useBoard } from "./useBoard";
import { useNarrow } from "./useWidth";
import "./board.css";

/**
 * The Board tab of the Crew view on the board route. Nothing is drawn while Crew Preview is off (a service with it off answers 404,
 * and so does one with no Crew routes: both read as off, and no other route is read). Reads only: nothing here cancels, re-runs,
 * reassigns or moves a card (Crew moves cards, never the person). The service's own text is drawn as plain text.
 */
export function BoardView() {
  return useCrewPreview() ? <Board /> : null;
}

function Board() {
  const crews = useFlowCrews();
  const [picked, setPicked] = useState<string | null>(null);
  if (crews.status === "loading") return <Loading label="Reading the board" rows={4} />;
  if (crews.status === "error" && !crews.data) return <ErrorState title="Can’t read the board" failure={crews.error} onRetry={() => void crews.reload()} />;
  const list = crews.data ?? [];
  if (list.length === 0) return <Empty title="No crews running" />;
  const crew = list.find((one) => one.id === picked) ?? list[0]!;
  return (
    <section className="bd-root" aria-label="Taskboard">
      <div className="bd-crew">
        <label htmlFor="bd-crew-pick">Crew</label>
        <select id="bd-crew-pick" value={crew.id} onChange={(event) => setPicked(event.target.value)}>
          {list.map((one) => (
            <option key={one.id} value={one.id}>
              {one.label} · {one.tasks} {one.tasks === 1 ? "task" : "tasks"}, {one.finished} finished
            </option>
          ))}
        </select>
      </div>
      <CrewBoard key={crew.id} crew={crew.id} />
    </section>
  );
}

function CrewBoard({ crew }: { crew: string }) {
  const [engine, setEngine] = useState<string | null>(null);
  const [kind, setKind] = useState<string | null>(null);
  const [overrides, setOverrides] = useState<Partial<Record<FlowColumnId, boolean>>>({});
  const [ref, narrow] = useNarrow();
  const { first, board, cardsOf, nextOf, showMore, busy, failure } = useBoard({ crew, engine, kind });

  if (first.status === "loading") return <Loading label="Reading the board" rows={4} />;
  if (first.status === "error" && !board) return <ErrorState title="Can’t read the board" failure={first.error} onRetry={() => void first.reload()} />;
  if (!board) return null;

  // Wide, every column but the two rails is open; narrow, Needs you and Running open first and the rest are collapsed with their counts.
  const isOpen = (spec: ColumnSpec) => overrides[spec.id] ?? (narrow ? spec.id === "needs_you" || spec.id === "running" : !spec.rail);

  return (
    <div ref={ref as React.RefObject<HTMLDivElement>} className="bd-body" data-narrow={narrow || undefined}>
      <div className="bd-chips" role="group" aria-label="Filter the board">
        <button type="button" className="bd-chip" aria-pressed={engine === null && kind === null} onClick={() => { setEngine(null); setKind(null); }}>
          All {board.total}
        </button>
        {board.engines.map((one) => (
          <button key={`e-${one.name}`} type="button" className="bd-chip" aria-pressed={engine === one.name} onClick={() => setEngine(engine === one.name ? null : one.name)}>
            {one.name} {one.count}
          </button>
        ))}
        {board.kinds.map((one) => (
          <button key={`k-${one.name}`} type="button" className="bd-chip" aria-pressed={kind === one.name} onClick={() => setKind(kind === one.name ? null : one.name)}>
            {one.name} {one.count}
          </button>
        ))}
      </div>
      {board.truncated ? (
        <p className="bd-note-line" role="note">
          This crew has more tasks than one read covers; the newest are shown.
        </p>
      ) : null}
      {board.total === 0 ? (
        <Empty title="No tasks yet">Cards are briefs the lead writes; a person can ask the lead for one.</Empty>
      ) : (
        <div className="bd-cols">
          {COLUMNS.map((spec) => {
            const column = board.columns.find((one) => one.column === spec.id);
            const count = column?.count ?? 0;
            const cards = cardsOf(spec.id);
            const open = isOpen(spec);
            const header = (
              <button type="button" className="bd-head" aria-expanded={open} onClick={() => setOverrides((now) => ({ ...now, [spec.id]: !open }))}>
                <span className={`bd-st bd-${spec.tone}`}>
                  {spec.working ? <i className="bd-spin" aria-hidden /> : null}
                  {spec.label}
                </span>
                <span className="bd-n">{count}</span>
              </button>
            );
            return (
              <section key={spec.id} className="bd-col" data-open={open || undefined} data-rail={(spec.rail && !open) || undefined} aria-label={`${spec.label}, ${count}`}>
                {header}
                {open ? (
                  <>
                    <ul className="bd-cards">
                      {cards.map((card) => (
                        <Card key={card.id} card={card} onChanged={() => void first.reload()} />
                      ))}
                    </ul>
                    {nextOf(spec.id) !== null ? (
                      <Button small busy={busy[spec.id]} onClick={() => void showMore(spec.id)}>
                        +{moreOf(count, cards.length)} more
                      </Button>
                    ) : null}
                    {failure?.column === spec.id ? <ErrorState compact title="Can’t read more cards" failure={failure.error} onRetry={() => void showMore(spec.id)} /> : null}
                  </>
                ) : null}
              </section>
            );
          })}
        </div>
      )}
    </div>
  );
}

function Card({ card, onChanged }: { card: FlowCard; onChanged(): void }) {
  const write = useTaskWrite({ id: card.id, name: card.name, allowCancel: card.can_cancel, allowRerun: card.can_rerun, onDone: onChanged });
  const needs = card.state === "needs_person";
  const model = modelLine(card);
  return (
    <li className="bd-card" data-needs={needs || undefined}>
      <div className="bd-k">
        <b>{card.kind || "task"}</b>
        <span>updated {ago(card.updated_at)}</span>
        <CardMenu card={card} actions={write} />
      </div>
      <div className="bd-t">{card.name || "Untitled task"}</div>
      <div className="bd-e">
        <b>{card.engine ?? "Not routed yet"}</b>
        {model ? <span>{model}</span> : null}
      </div>
      {card.owner ? <div className="bd-ln">owner {card.owner}</div> : null}
      {needs && card.reason ? <div className="bd-ln bd-amber">{card.reason}</div> : null}
      <WriteStatus id={card.id} op={write.op} />
      {write.dialog}
    </li>
  );
}
