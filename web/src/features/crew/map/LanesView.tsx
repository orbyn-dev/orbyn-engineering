import { useState } from "react";
import { ErrorState, Empty, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { useRefresh } from "../flow/live/useFlow";
import { useNarrow } from "../board/useWidth";
import { CrewPick } from "./CrewPick";
import { MessageLines } from "./MapView";
import { clock, memberFilterable, memberName, modelWord, ordered, swimLayout } from "./model";
import { useCrewFacts, useCrewMap, useCrewMessages } from "./useMap";
import "./map.css";

/**
 * The Lanes tab of the Crew view: the swim lanes of Crew Designs' lanes-main, drawn from the map and message reads. One lane per
 * member, the lead first, over a time axis taken from the messages; each message is an arrow from its sender's lane to its
 * receiver's, labelled with how it arrived and its fixed kind; each worker's brief outcome is a bar over its run. The shared
 * board's facts sit beside them, and the message list of the beta stays under the lanes. A message is one line and never its
 * text; a fact is its key, class, author and revision and never its value. A lane's level is in no read, so it is a Planned chip
 * naming the missing read. Nothing is drawn while Crew Preview is off. Reads only.
 */
export function LanesView() {
  return useCrewPreview() ? (
    <section className="mp-root" aria-label="Conversation lanes">
      <CrewPick label="lanes" id="ln-crew-pick">
        {(crew) => <CrewLanes key={crew} crew={crew} />}
      </CrewPick>
    </section>
  ) : null;
}

function CrewLanes({ crew }: { crew: string }) {
  const [measure, narrow] = useNarrow();
  const map = useCrewMap(crew);
  const [member, setMember] = useState<string | null>(null);
  const messages = useCrewMessages(crew, member);
  const facts = useCrewFacts(crew);
  useRefresh(map.reload, map.status !== "ready");
  useRefresh(messages.first.reload, messages.first.status !== "ready" || messages.rows.length > (messages.first.data?.messages.length ?? 0));
  useRefresh(facts.reload, facts.status !== "ready");

  if (map.status === "loading") return <Loading label="Reading the lanes" rows={4} />;
  if (map.status === "error" && !map.data) return <ErrorState title="Can’t read the lanes" failure={map.error} onRetry={() => void map.reload()} />;
  const nodes = ordered(map.data!.nodes);
  const swim = narrow ? null : swimLayout(nodes, messages.rows);
  return (
    <div className="mp-body" ref={measure}>
      {swim === null ? (
        <ul className="mp-lanes" aria-label="Lanes">
          {nodes.map((node) => (
            <li key={node.member}>
              <b>{memberName(node)}</b> <span className="mp-sub">{[node.engine, modelWord(node)].filter(Boolean).join(" · ")}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="mp-split">
        <div className="mp-col">
          <div className="mp-tool">
            <label>
              <span className="sr-only">Member</span>
              <select value={member ?? ""} onChange={(event) => setMember(event.target.value === "" ? null : event.target.value)}>
                <option value="">Everyone</option>
                {nodes.map((node) => (
                  <option key={node.member} value={node.member} disabled={!memberFilterable(node.member)}>
                    {memberFilterable(node.member) ? node.member : node.member + " (can’t be filtered)"}
                  </option>
                ))}
              </select>
            </label>
            {messages.first.data ? <span className="mp-count">{messages.rows.length} of {messages.total} shown</span> : null}
          </div>
          {swim ? (
            <section className="mp-swim" role="group" aria-label="Swim lanes">
              <div className="mp-ltl" style={{ width: swim.width, height: swim.height }}>
                <svg className="mp-lsv" width={swim.width} height={swim.height} aria-hidden="true">
                  {swim.rows.map((row, at) => (
                    <path key={"lane-" + at} className="mp-ln" d={"M140," + (row.y + 22) + " H" + swim.width} />
                  ))}
                  {swim.ticks.map((tick, at) => (
                    <path key={"grid-" + at} className="mp-ngth" d={"M" + tick.x + ",20 V" + (swim.height - 10)} strokeDasharray="2 5" />
                  ))}
                  {swim.arrows.map((arrow, at) => (
                    <path key={"arrow-" + at} className={"mp-arrow" + (arrow.delivery === "live" ? " mp-arrow-live" : "")} d={arrow.d} />
                  ))}
                  {swim.arrows.map((arrow, at) => (
                    <path key={"head-" + at} className={"mp-arrowhead" + (arrow.delivery === "live" ? " mp-arrowhead-live" : "")} d={arrow.head} />
                  ))}
                </svg>
                <ul className="mp-lrows" aria-label="Lanes">
                  {swim.rows.map((row) => (
                    <li key={row.node.member} className="mp-lrow" style={{ top: row.y }}>
                      <b>{memberName(row.node)}</b>
                      <small>{[row.node.engine, modelWord(row.node)].filter(Boolean).join(" · ")}</small>
                    </li>
                  ))}
                </ul>
                {swim.ticks.map((tick, at) => (
                  <span key={"tick-" + at} className="mp-tick" style={{ left: tick.x, top: 4 }}>
                    {tick.label}
                  </span>
                ))}
                {swim.bars.map((bar, at) => (
                  <div key={"bar-" + at} className="mp-lbar" style={{ left: bar.x, width: bar.w, top: bar.y }}>
                    {bar.text}
                  </div>
                ))}
                {swim.arrows.map((arrow, at) =>
                  arrow.label === "" ? null : (
                    <span key={"label-" + at} className={"mp-lmk" + (arrow.delivery === "live" ? " mp-lmk-live" : "")} style={{ left: arrow.x, top: arrow.y }} title={arrow.title}>
                      {arrow.delivery ? <b>{arrow.delivery}</b> : null}
                      {arrow.delivery ? " · " : ""}
                      {arrow.kind}
                    </span>
                  ),
                )}
                <span className="mp-swim-pl">
                  <span className="mp-pl crew-pv" title="Planned: no route reports a lane’s level">
                    Planned
                  </span>
                  <span>A lane’s level needs a read that reports it.</span>
                </span>
              </div>
              <div className="mp-legend">
                <span>“live” means handed to a turn in progress (never for an engine that can’t be steered mid-turn); “next turn” means queued, or sent with the next prompt; “tool result” means a held message returned in the receiver’s next Crew tool result; “held” means no running turn took it, so it waits for the next prompt or tool call; “failed” means delivery failed; no delivery is shown when there is no delivery row or the receivers disagree</span>
              </div>
            </section>
          ) : null}
          {messages.first.status === "loading" ? <Loading label="Reading messages" rows={3} /> : null}
          {messages.first.status === "error" && !messages.first.data ? <ErrorState title="Can’t read messages" failure={messages.first.error} onRetry={() => void messages.first.reload()} /> : null}
          {messages.first.data && messages.total === 0 ? (
            <Empty title="No messages yet">Workers talk to each other and to the lead through Crew, and every message appears here, with how it arrived and when. Nothing has been said in this crew so far.</Empty>
          ) : null}
          <h3 className="mp-h">Messages</h3>
          {messages.first.data && messages.total > 0 ? <MessageLines rows={messages.rows} total={messages.total} /> : null}
          {messages.failed ? <ErrorState compact title="Can’t read more messages" failure={messages.failed} onRetry={() => void messages.showMore()} /> : null}
          {messages.next !== null ? (
            <Button small busy={messages.busy} onClick={() => void messages.showMore()}>
              +{Math.max(0, messages.total - messages.rows.length)} more
            </Button>
          ) : null}
        </div>
        <section className="mp-board" aria-label="Shared board">
          <h3>Shared board</h3>
          {facts.status === "loading" ? <Loading label="Reading the shared board" rows={2} /> : null}
          {facts.status === "error" && !facts.data ? <ErrorState compact title="Can’t read the shared board" failure={facts.error} onRetry={() => void facts.reload()} /> : null}
          {facts.data && facts.data.truncated ? (
            <p className="mp-note" role="note">
              A list was cut at its cap; the newest are shown.
            </p>
          ) : null}
          {facts.data && facts.data.facts.length === 0 ? <p className="mp-muted">No facts yet</p> : null}
          {facts.data && facts.data.facts.length > 0 ? (
            <ul className="mp-facts-list" aria-label="Facts">
              {facts.data.facts.map((fact) => (
                <li key={fact.key + "-" + fact.rev}>
                  <b>{fact.key}</b>
                  <span className="mp-sub">
                    {fact.class} · {fact.author} · rev {fact.rev} · {clock(fact.at)}
                  </span>
                  {fact.verified ? (
                    <span className="mp-st mp-ok" title="The fact names evidence">
                      Verified
                    </span>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : null}
        </section>
      </div>
    </div>
  );
}
