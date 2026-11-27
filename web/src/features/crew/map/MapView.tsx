import { Ellipsis, GitBranch, Lock, MessageSquare } from "lucide-react";
import { useRef, useState } from "react";
import type { FlowMessage } from "../../../generated/FlowMessage";
import type { FlowNode } from "../../../generated/FlowNode";
import { ErrorState, Empty, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { useBoardFocus } from "../board/focus";
import { useNarrow } from "../board/useWidth";
import { ago } from "../board/model";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { useRefresh } from "../flow/live/useFlow";
import { CrewPick } from "./CrewPick";
import { clock, deliveryWord, engineProblem, engineStatusWord, kindLabel, leaseWord, modelWord, elapsedWord, ordered, quietWord, subagentsWord, treeLayout, whyLine, workerState, crewSummary } from "./model";
import { useCrewMap, useCrewMessages } from "./useMap";
import "./map.css";

/** Past this many workers the drawn tree is a list (crew-map-many). */
const MANY = 8;

/**
 * The Map tab of the Crew view, on the map route. Wide, with up to eight workers, it is the drawn tree of Crew Designs' crew-map;
 * in a narrow panel or window (below 640 px) or with more than eight workers it is the nested list of the narrow frames (the lead,
 * then each worker under it), never the drawn tree. In the tree the lead
 * sits first, each worker that says the lead started it hangs under the lead with a solid line, each message between members is a
 * dashed line with its own label and delivery, and each worker's subagent report sits beside it. Nothing is drawn while Crew
 * Preview is off (a service with it off answers 404, and so does one with no Crew routes: both read as off). Reads only. The
 * service's own text is drawn as plain text; what a read does not carry (a worker's token count, a branch, a subagent's own row,
 * a lane level) is a Planned chip naming the missing read, never a value.
 */
export function MapView() {
  return useCrewPreview() ? (
    <section className="mp-root" aria-label="Crew map">
      <CrewPick label="map" id="mp-crew-pick">
        {(crew) => <CrewMap key={crew} crew={crew} />}
      </CrewPick>
    </section>
  ) : null;
}

function CrewMap({ crew }: { crew: string }) {
  const [measure, narrow] = useNarrow();
  const read = useCrewMap(crew);
  const [member, setMember] = useState<string | null>(null);
  useRefresh(read.reload, read.status !== "ready");
  if (read.status === "loading") return <Loading label="Reading the map" rows={4} />;
  if (read.status === "error" && !read.data) return <ErrorState title="Can’t read the map" failure={read.error} onRetry={() => void read.reload()} />;
  const map = read.data!;
  const nodes = ordered(map.nodes);
  const workers = nodes.filter((node) => node.role !== "lead");
  const selected = nodes.find((node) => node.member === member) ?? null;
  const problems = map.engines.flatMap((row) => {
    const line = engineProblem(row.engine, row.status);
    return line ? [{ engine: row.engine, line }] : [];
  });
  const summary = crewSummary(nodes);
  const nested = narrow || workers.length > MANY;
  const tree = nested ? null : treeLayout(nodes, map.edges);
  const pick = (id: string) => setMember(id === member ? null : id);
  return (
    <div className="mp-body" ref={measure}>
      {map.truncated ? (
        <p className="mp-note" role="note">
          A list was cut at its cap; the newest are shown.
        </p>
      ) : null}
      {summary ? <p className="mp-summary" data-testid="map-summary">{summary}</p> : null}
      <ul className="mp-engines" aria-label="Engines">
        {map.engines.map((row) => (
          <li key={row.engine}>
            {row.engine} · {engineStatusWord(row.status)}
          </li>
        ))}
      </ul>
      {problems.map((one) => (
        <p key={one.engine} className="mp-banner" role="status">
          {one.line}
        </p>
      ))}
      {workers.length === 0 ? (
        <Empty title="No workers yet">The lead hands tasks to other engines, and each worker appears here as it starts, with its subagents. Ask the lead in its thread, or start from a template.</Empty>
      ) : null}
      <div className="mp-split">
        {tree === null ? (
          <ul className="mp-nest" aria-label="Crew">
            {nodes.map((node) => (
              <Row key={node.member} node={node} selected={node.member === member} onPick={() => pick(node.member)} />
            ))}
          </ul>
        ) : (
        <div className="mp-canvas">
          <div className="mp-map" style={{ width: tree.width, height: tree.height }}>
            <svg className="mp-ed" width={tree.width} height={tree.height} aria-hidden="true">
              {tree.starts.map((path, at) => (
                <path key={"start-" + at} className="mp-start" d={path.d} />
              ))}
              {tree.talks.map((edge, at) => (
                <path key={"talk-" + at} className="mp-talk" d={edge.d} />
              ))}
            </svg>
            <ul className="mp-tree" aria-label="Crew">
              {tree.cards.map((card) => (
                <li key={card.node.member} className="mp-node" data-worker={card.node.role !== "lead" || undefined} style={{ left: card.x, top: card.y, width: card.w }}>
                  <Card node={card.node} selected={card.node.member === member} onPick={() => pick(card.node.member)} />
                </li>
              ))}
            </ul>
            <div role="group" aria-label="Subagent reports">
              {tree.subs.map((sub) => (
                <Sub key={sub.node.member} node={sub.node} x={sub.x} y={sub.y} w={sub.w} />
              ))}
            </div>
            {tree.talks.map((edge, at) =>
              edge.label === "" ? null : (
                <span key={"label-" + at} className={"mp-elab" + (edge.center ? "" : " mp-elab-start")} style={{ left: edge.x, top: edge.y }} title={edge.title}>
                  <b>
                    {edge.from} → {edge.to}
                  </b>{" "}
                  {edge.label}
                  {edge.delivery ? " · " + edge.delivery : ""}
                </span>
              ),
            )}
          </div>
          <div className="mp-legend">
            <span>
              <svg width="26" height="8" aria-hidden="true">
                <path className="mp-start" d="M0 4H26" />
              </svg>
              started by
            </span>
            <span>
              <svg width="26" height="8" aria-hidden="true">
                <path className="mp-talk" d="M0 4H26" />
              </svg>
              talking to
            </span>
            <span>“live” means handed to a turn in progress or to a waiting crew_ask call; “next turn” means queued, or sent with the next prompt</span>
          </div>
        </div>
        )}
        {selected ? <Detail crew={crew} node={selected} onClose={() => setMember(null)} /> : null}
      </div>
    </div>
  );
}

/** One row of the nested list of the narrow frames and of a crew past eight workers. */
function Row({ node, selected, onPick }: { node: FlowNode; selected: boolean; onPick(): void }) {
  const state = workerState(node);
  const why = whyLine(node.working);
  const quiet = quietWord(node.quiet);
  const subs = subagentsWord(node.subagents);
  const model = modelWord(node);
  return (
    <li className="mp-nli" data-worker={node.role !== "lead" || undefined}>
      <button type="button" className="mp-row" aria-pressed={selected} onClick={onPick}>
        <span className="mp-main">
          <b>{node.engine}</b>
          {model ? <span className="mp-sub">{model}</span> : null}
        </span>
        <span className={`mp-st mp-${state.tone}`}>{state.text}</span>
        {quiet ? <span className={`mp-st mp-${quiet.tone}`}>{quiet.text}</span> : null}
        {node.engine_status !== "ready" ? <span className="mp-st mp-neutral">{engineStatusWord(node.engine_status)}</span> : null}
        {node.outcome ? <span className="mp-outcome">{node.outcome}</span> : null}
        {why ? <span className="mp-why">{why}</span> : null}
        <span className="mp-meta">
          {node.unread > 0 ? <span>{node.unread} unread</span> : null}
          {subs ? <span>{subs}</span> : null}
          {node.updated_at ? <span>updated {ago(node.updated_at)}</span> : null}
        </span>
      </button>
    </li>
  );
}

/** One node of the wide tree. Fields no read carries are chips naming the missing read; nothing is guessed. */
function Card({ node, selected, onPick }: { node: FlowNode; selected: boolean; onPick(): void }) {
  const state = workerState(node);
  const why = whyLine(node.working);
  const quiet = quietWord(node.quiet);
  const model = modelWord(node);
  const elapsed = elapsedWord(node);
  const line = [model, elapsed].filter(Boolean).join(" · ");
  const className = "mp-card" + (state.tone === "warn" ? " mp-card-needs" : state.tone === "err" ? " mp-card-fail" : "");
  return (
    <button type="button" className={className} aria-pressed={selected} onClick={onPick}>
      <span className="mp-card-h">
        <b>{node.engine}</b>
        {node.unread > 0 ? (
          <span className="mp-unread" title={node.unread + " unread"}>
            <MessageSquare size={12} aria-hidden />
            <span className="sr-only">{node.unread} unread</span>
            <span aria-hidden>{node.unread}</span>
          </span>
        ) : null}
        <span className={"mp-st mp-" + state.tone}>
          {state.tone === "info" ? <i className="mp-spin" aria-hidden /> : null}
          {state.text}
        </span>
      </span>
      {line ? <span className="mp-card-model">{line}</span> : null}
      <span className="mp-card-model">
        tokens
        <span className="mp-pl crew-pv" title="Planned: no Map route reports per-worker tokens yet">
          Planned
        </span>
      </span>
      {node.outcome ? <span className="mp-card-outcome" title={node.outcome}>{node.outcome}</span> : null}
      {why ? <span className="mp-why">{why}</span> : null}
      <span className="mp-card-meta">
        {node.engine_status !== "ready" ? <span className="mp-st mp-neutral">{engineStatusWord(node.engine_status)}</span> : null}
        {quiet ? <span className={"mp-st mp-" + quiet.tone}>{quiet.text}</span> : null}
        <span>
          <GitBranch size={12} aria-hidden />
          branch
          <span className="mp-pl crew-pv" title="Planned: no Map route reports a branch">
            Planned
          </span>
        </span>
        <span>
          <Lock size={12} aria-hidden />
          {leaseWord(node)}
        </span>
      </span>
    </button>
  );
}

/**
 * A worker's subagent report. The read says whether the engine reports its subagents, never which ones: the row that would name
 * them is Planned, and the not-reported card says so without drawing an empty tree.
 */
function Sub({ node, x, y, w }: { node: FlowNode; x: number; y: number; w: number }) {
  const reported = node.subagents === "reported";
  return (
    <div className={"mp-subcard" + (reported ? "" : " mp-subcard-none")} data-member={node.member} style={{ left: x, top: y, width: w }}>
      <span className="mp-card-h">
        <b>{node.engine} subagents</b>
      </span>
      <span className="mp-card-model">{subagentsWord(node.subagents) ?? "Subagents reported"}</span>
      {reported ? (
        <>
          <span className="mp-card-model">
            <span className="mp-pl crew-pv" title="Planned: no Map route names a worker’s subagents">
              Planned
            </span>
          </span>
          <span className="mp-card-why">A read that names them is not built.</span>
        </>
      ) : (
        <span className="mp-card-why">Not an empty tree: not reported.</span>
      )}
    </div>
  );
}

const AFTER = "Planned";
/** The map does not carry a task's cancel or re-run flags, so it does not guess them: those two are inert here, and the Board card has them. */
const NOT_HERE = "Use the Board card";

const inert = (label: string, note: string): MenuItem => ({
  label,
  disabled: true,
  title: label + ": " + note,
  content: (
    <>
      {label} <span className="mp-note-inline">{note}</span>
    </>
  ),
});

function Detail({ crew, node, onClose }: { crew: string; node: FlowNode; onClose(): void }) {
  const messages = useCrewMessages(crew, node.member);
  // The same shared refresh as the other lists: every 30 s while the window is visible, paused while hidden, only for what is shown.
  useRefresh(messages.first.reload, messages.first.status !== "ready" || !messages.filterable || messages.rows.length > (messages.first.data?.messages.length ?? 0));
  const state = workerState(node);
  const model = modelWord(node);
  const button = useRef<HTMLButtonElement>(null);
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const items: MenuItem[] = [
    inert("Open thread", AFTER),
    ...(node.task ? [inert("Cancel", NOT_HERE), inert("Re-run checks", NOT_HERE)] : []),
    inert("Reassign engine", AFTER),
    inert("Message into a lane", AFTER),
    inert("Add reviewer lane", AFTER),
    ...(node.state === "needs_person" ? [inert("Answer a worker’s question", AFTER)] : []),
    inert("Propose a check", AFTER),
  ];
  return (
    <aside className="mp-detail" aria-label={node.engine + ", details"}>
      <div className="mp-dhead">
        <h3>
          {node.engine}
          {node.role === "lead" ? " · lead" : ""}
        </h3>
        <span className={"mp-st mp-" + state.tone}>{state.text}</span>
        <button
          ref={button}
          type="button"
          className="mp-more"
          aria-label="Actions for this member"
          aria-haspopup="menu"
          aria-expanded={at !== null}
          onClick={() => {
            const box = button.current?.getBoundingClientRect();
            setAt(at ? null : { x: box?.left ?? 0, y: (box?.bottom ?? 0) + 4 });
          }}
        >
          <Ellipsis size={14} aria-hidden />
        </button>
        {node.task ? (
          <Button small onClick={() => useBoardFocus.getState().ask(crew, node.task!)}>
            Open on Board
          </Button>
        ) : null}
        <Button small onClick={onClose}>
          Close
        </Button>
      </div>
      <Presence>{at ? <Menu x={at.x} y={at.y} label="Member actions" items={items} onClose={() => setAt(null)} /> : null}</Presence>
      <dl className="mp-facts">
        {node.outcome ? (
          <>
            <dt>Outcome</dt>
            <dd>{node.outcome}</dd>
          </>
        ) : null}
        {model ? (
          <>
            <dt>Model</dt>
            <dd>{model}</dd>
          </>
        ) : null}
        <dt>Lease</dt>
        <dd>{leaseWord(node)}</dd>
        {node.started_by ? (
          <>
            <dt>Started by</dt>
            <dd>{node.started_by}</dd>
          </>
        ) : null}
        <dt>Unread</dt>
        <dd>{node.unread}</dd>
        {node.updated_at ? (
          <>
            <dt>Updated</dt>
            <dd>{ago(node.updated_at)}</dd>
          </>
        ) : null}
        <dt>Subagents</dt>
        <dd>{subagentsWord(node.subagents) ?? "Reported by the engine"}</dd>
      </dl>
      <h4>Messages</h4>
      {messages.first.status === "loading" ? <Loading label="Reading messages" rows={2} /> : null}
      {messages.first.status === "error" && !messages.first.data ? <ErrorState compact title="Can’t read messages" failure={messages.first.error} onRetry={() => void messages.first.reload()} /> : null}
      {!messages.filterable ? <p className="mp-muted">Messages can’t be filtered for this member: its name is withheld.</p> : null}
      {messages.first.data && messages.filterable ? <MessageLines rows={messages.rows} total={messages.total} /> : null}
      {messages.next !== null ? (
        <Button small busy={messages.busy} onClick={() => void messages.showMore()}>
          +{Math.max(0, messages.total - messages.rows.length)} more
        </Button>
      ) : null}
    </aside>
  );
}

/** One line a message: the time, who sent it, who it went to, its kind and how it was delivered. Never its text. */
export function MessageLines({ rows, total }: { rows: FlowMessage[]; total: number }) {
  if (rows.length === 0 && total === 0) return <p className="mp-muted">No messages</p>;
  return (
    <ul className="mp-lines" aria-label="Messages">
      {rows.map((one) => {
        const delivery = deliveryWord(one.delivery);
        return (
          <li key={one.id}>
            {clock(one.at)} · {one.from} → {one.to} · {kindLabel(one.kind, one.label)}
            {delivery ? " · " + delivery : ""}
          </li>
        );
      })}
    </ul>
  );
}
