import { Ellipsis } from "lucide-react";
import { useRef, useState } from "react";
import type { FlowMessage } from "../../../generated/FlowMessage";
import type { FlowNode } from "../../../generated/FlowNode";
import { ErrorState, Empty, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { ago } from "../board/model";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { useRefresh } from "../flow/live/useFlow";
import { CrewPick } from "./CrewPick";
import { clock, deliveryWord, engineProblem, engineStatusWord, kindLabel, leaseWord, modelWord, ordered, quietWord, stateWord, subagentsWord } from "./model";
import { useCrewMap, useCrewMessages } from "./useMap";
import "./map.css";

/**
 * The Map tab of the Crew view, on the map route. It draws the crew as the nested list of the narrow frames (the lead, then each
 * worker under it, one level), at every width: the drawn tree of the wide frames is not built. Nothing is drawn while Crew Preview
 * is off (a service with it off answers 404, and so does one with no Crew routes: both read as off). Reads only. The service's own
 * text is drawn as plain text, and no field the routes do not carry (tokens, branch, level, a message's text) is drawn.
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
  return (
    <div className="mp-body">
      {map.truncated ? (
        <p className="mp-note" role="note">
          A list was cut at its cap; the newest are shown.
        </p>
      ) : null}
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
        <ul className="mp-tree" aria-label="Crew">
          {nodes.map((node) => (
            <Row key={node.member} node={node} selected={node.member === member} onPick={() => setMember(node.member === member ? null : node.member)} />
          ))}
        </ul>
        {selected ? <Detail crew={crew} node={selected} onClose={() => setMember(null)} /> : null}
      </div>
    </div>
  );
}

function Row({ node, selected, onPick }: { node: FlowNode; selected: boolean; onPick(): void }) {
  const state = stateWord(node);
  const quiet = quietWord(node.quiet);
  const subs = subagentsWord(node.subagents);
  const model = modelWord(node);
  return (
    <li className="mp-node" data-worker={node.role !== "lead" || undefined}>
      <button type="button" className="mp-row" aria-pressed={selected} onClick={onPick}>
        <span className="mp-main">
          <b>{node.engine}</b>
          {model ? <span className="mp-sub">{model}</span> : null}
        </span>
        <span className={`mp-st mp-${state.tone}`}>{state.text}</span>
        {quiet ? <span className={`mp-st mp-${quiet.tone}`}>{quiet.text}</span> : null}
        {node.engine_status !== "ready" ? <span className="mp-st mp-neutral">{engineStatusWord(node.engine_status)}</span> : null}
        {node.outcome ? <span className="mp-outcome">{node.outcome}</span> : null}
        <span className="mp-meta">
          {node.unread > 0 ? <span>{node.unread} unread</span> : null}
          {subs ? <span>{subs}</span> : null}
          {node.updated_at ? <span>updated {ago(node.updated_at)}</span> : null}
        </span>
      </button>
    </li>
  );
}

const AFTER = "After the beta";
/** The map does not carry a task's cancel or re-run flags, so it does not guess them: those two are inert here, and the Board card has them. */
const NOT_HERE = "Use the card on the Board";

const inert = (label: string, note: string): MenuItem => ({
  label,
  disabled: true,
  title: `${label}: ${note}`,
  content: (
    <>
      {label} <span className="mp-note-inline">{note}</span>
    </>
  ),
});

function Detail({ crew, node, onClose }: { crew: string; node: FlowNode; onClose(): void }) {
  const messages = useCrewMessages(crew, node.member);
  const state = stateWord(node);
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
    <aside className="mp-detail" aria-label={`${node.engine}, details`}>
      <div className="mp-dhead">
        <h3>
          {node.engine}
          {node.role === "lead" ? " · lead" : ""}
        </h3>
        <span className={`mp-st mp-${state.tone}`}>{state.text}</span>
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
        <dt>Subagents</dt>
        <dd>{subagentsWord(node.subagents) ?? "Reported by the engine"}</dd>
      </dl>
      <h4>Messages</h4>
      {messages.first.status === "loading" ? <Loading label="Reading messages" rows={2} /> : null}
      {messages.first.status === "error" && !messages.first.data ? <ErrorState compact title="Can’t read messages" failure={messages.first.error} onRetry={() => void messages.first.reload()} /> : null}
      {messages.first.data ? <MessageLines rows={messages.rows} total={messages.total} /> : null}
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
            {delivery ? ` · ${delivery}` : ""}
          </li>
        );
      })}
    </ul>
  );
}
