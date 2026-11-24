import { useEffect, useRef, useState } from "react";
import type { FlowCard } from "../../../generated/FlowCard";
import type { FlowColumnId } from "../../../generated/FlowColumnId";
import { useConnection } from "../../../product/state/connection";
import { ErrorState, Empty, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { RESUMED_CARD, wasResumed } from "../flow/live/model";
import { WORKER_MAY_RUN } from "../needsyou/assemble";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { useFlowCrews } from "../flow/live/useFlow";
import { useTaskWrite, WriteStatus } from "../writes/TaskWrite";
import { runWrite } from "../writes/store";
import { CardMenu } from "./CardMenu";
import { useBoardFocus } from "./focus";
import { ago, checksLine, COLUMNS, modelLine, moreOf, waitingLine, workingLine, type ColumnSpec } from "./model";
import { useBoard } from "./useBoard";
import { useNarrow } from "./useWidth";
import "./board.css";

/**
 * The Board tab of the Crew view on the board route. Nothing is drawn while Crew Preview is off (a service with it off answers 404,
 * and so does one with no Crew routes: both read as off, and no other route is read). The writes are the person's own: a card's
 * Stop again, and its menu's Cancel and Re-run checks. Nothing here reassigns or moves a card (Crew moves cards, never the person).
 * The service's own text is drawn as plain text.
 */
export function BoardView() {
  // A request the Board never consumed must not wait for a later visit. React's dev double-mount runs this cleanup too, so
  // the clear waits a tick and is skipped when the Board is already back.
  const live = useRef(false);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
      setTimeout(() => {
        if (!live.current) useBoardFocus.getState().clear();
      }, 0);
    };
  }, []);
  return useCrewPreview() ? <Board /> : null;
}

function Board() {
  const crews = useFlowCrews();
  // A map node may have asked for one of its crew's cards: that crew is the one shown first.
  const [picked, setPicked] = useState<string | null>(() => useBoardFocus.getState().crew);
  const pick = (id: string) => {
    // Choosing another crew drops any card still asked for.
    useBoardFocus.getState().clear();
    setPicked(id);
  };
  if (crews.status === "loading") return <Loading label="Reading the board" rows={4} />;
  if (crews.status === "error" && !crews.data) return <ErrorState title="Can’t read the board" failure={crews.error} onRetry={() => void crews.reload()} />;
  const list = crews.data ?? [];
  if (list.length === 0) return <Empty title="No crews running" />;
  const crew = list.find((one) => one.id === picked) ?? list[0]!;
  return (
    <section className="bd-root" aria-label="Taskboard">
      <div className="bd-crew">
        <label htmlFor="bd-crew-pick">Crew</label>
        <select id="bd-crew-pick" value={crew.id} onChange={(event) => pick(event.target.value)}>
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
  const asked = useBoardFocus((state) => (state.crew === crew ? state.task : null));
  const [missing, setMissing] = useState(false);

  // A card asked for by a map node: open its column, scroll to it and focus it; if it is not on the page read, say so.
  useEffect(() => {
    if (!asked || !board) return;
    const found = COLUMNS.find((spec) => cardsOf(spec.id).some((card) => card.id === asked));
    if (!found) {
      setMissing(true);
      useBoardFocus.getState().clear();
      return;
    }
    setOverrides((now) => ({ ...now, [found.id]: true }));
    const frame = requestAnimationFrame(() => {
      const element = [...document.querySelectorAll<HTMLElement>("[data-task]")].find((one) => one.getAttribute("data-task") === asked);
      element?.scrollIntoView?.({ block: "center" });
      element?.focus();
      useBoardFocus.getState().clear();
    });
    return () => cancelAnimationFrame(frame);
    // `cardsOf` is rebuilt every render; the cards are in `board`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [asked, board]);

  if (first.status === "loading") return <Loading label="Reading the board" rows={4} />;
  if (first.status === "error" && !board) return <ErrorState title="Can’t read the board" failure={first.error} onRetry={() => void first.reload()} />;
  if (!board) return null;

  // Wide, every column but the two rails is open; narrow, Needs you and Running open first and the rest are collapsed with their counts.
  const isOpen = (spec: ColumnSpec) => overrides[spec.id] ?? (narrow ? spec.id === "needs_you" || spec.id === "running" : !spec.rail);

  return (
    <div ref={ref} className="bd-body" data-narrow={narrow || undefined}>
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
      {missing ? (
        <p className="bd-note-line" role="status">
          That task’s card isn’t on the current page of its column. Use “+N more” in its column to find it.
        </p>
      ) : null}
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
                    {spec.id === "needs_you" && count === 0 ? (
                      <div className="bd-empty">Nothing waits for you. The Inbox, the Review queue and the sidebar say the same.</div>
                    ) : null}
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
  const remote = useConnection((state) => state.remote);
  // A task refused at its start waits for no decision: it says so, and is not drawn as one that does.
  // A task whose worker could not be stopped is cancelled and final: it says so instead of carrying a question's amber line, and
  // keeps the Needs you card's amber ground.
  const stuck = card.needs_you === "could_not_stop";
  // The one needs-you field names a waiting card wherever its run state sits (a merge that waits belongs to a passed task); a
  // dismissed field leaves the task reading needs_person. A stop kind (trip wire, worker stopped twice) is not a wait.
  const needs = !stuck && !card.could_not_start && (card.state === "needs_person" || card.needs_you === "task_needs_person" || card.needs_you === "merge_waiting" || card.needs_you === "manual_check");
  const waiting = needs ? waitingLine(card) : null;
  const working = needs || stuck || card.could_not_start ? null : workingLine(card.working);
  const checks = checksLine(card);
  const model = modelLine(card);
  const stopOff = remote ? "Not available from another computer" : write.op?.phase === "busy" ? "Waiting for the answer" : null;
  return (
    <li className="bd-card" data-needs={needs || stuck || undefined} data-task={card.id} tabIndex={-1}>
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
      {checks ? <div className="bd-ln">{checks}</div> : null}
      {wasResumed(card) ? <div className="bd-ln bd-resumed">↻ {RESUMED_CARD}</div> : null}
      {card.could_not_start ? <div className="bd-ln"><b>Could not start</b> · {card.reason ?? "The worker never ran."}</div> : null}
      {stuck ? (
        <>
          <div className="bd-ln bd-amber"><b>Could not stop</b> · {card.reason?.trim() || WORKER_MAY_RUN}</div>
          <div className="bd-ln">
            <Button
              small
              tone="primary"
              aria-label={`Stop again: ${card.name}`}
              title={stopOff ?? undefined}
              disabled={stopOff !== null}
              onClick={() => void runWrite("cancel", card.id, "", onChanged)}
            >
              Stop again
            </Button>
          </div>
        </>
      ) : null}
      {working ? <div className="bd-ln bd-working">{working}</div> : null}
      {waiting ? (
        <div className="bd-ln bd-amber">
          {waiting.lead ? <b>{waiting.lead}</b> : null}
          {waiting.lead && waiting.text ? " · " : null}
          {waiting.text}
        </div>
      ) : null}
      <WriteStatus id={card.id} op={write.op} />
      {write.dialog}
    </li>
  );
}
