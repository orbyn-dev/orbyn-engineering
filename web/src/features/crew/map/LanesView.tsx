import { useState } from "react";
import { ErrorState, Empty, Loading } from "../../../product/ui/states";
import { Button } from "../../../ui/controls";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { useRefresh } from "../flow/live/useFlow";
import { CrewPick } from "./CrewPick";
import { MessageLines } from "./MapView";
import { clock, memberFilterable, modelWord, ordered } from "./model";
import { useCrewFacts, useCrewMap, useCrewMessages } from "./useMap";
import "./map.css";

/**
 * The Lanes tab of the Crew view: who has written to whom, and the facts on the shared board. It is the message list of the narrow
 * lanes frame (time order, who to whom, kind and how it arrived) beside the board's facts, at every width: the swim lanes with arrows
 * between them are not built. A message is one line and never its text; a fact is its key, class, author and revision and never its
 * value. Nothing is drawn while Crew Preview is off. Reads only.
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
  return (
    <div className="mp-body">
      <ul className="mp-lanes" aria-label="Lanes">
        {nodes.map((node) => (
          <li key={node.member}>
            <b>{node.role === "lead" ? "Lead" : node.member}</b> <span className="mp-sub">{[node.engine, modelWord(node)].filter(Boolean).join(" · ")}</span>
          </li>
        ))}
      </ul>
      <div className="mp-split">
        <div className="mp-col">
          <div className="mp-tool">
            <label>
              <span className="sr-only">Member</span>
              <select value={member ?? ""} onChange={(event) => setMember(event.target.value === "" ? null : event.target.value)}>
                <option value="">Everyone</option>
                {nodes.map((node) => (
                  <option key={node.member} value={node.member} disabled={!memberFilterable(node.member)}>
                    {memberFilterable(node.member) ? node.member : `${node.member} (can’t be filtered)`}
                  </option>
                ))}
              </select>
            </label>
            {messages.first.data ? <span className="mp-count">{messages.rows.length} of {messages.total} shown</span> : null}
          </div>
          {messages.first.status === "loading" ? <Loading label="Reading messages" rows={3} /> : null}
          {messages.first.status === "error" && !messages.first.data ? <ErrorState title="Can’t read messages" failure={messages.first.error} onRetry={() => void messages.first.reload()} /> : null}
          {messages.first.data && messages.total === 0 ? (
            <Empty title="No messages yet">Workers talk to each other and to the lead through Crew, and every message appears here, with how it arrived and when. Nothing has been said in this crew so far.</Empty>
          ) : null}
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
                <li key={`${fact.key}-${fact.rev}`}>
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
