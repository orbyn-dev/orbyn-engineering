import { useEffect, useState } from "react";
import { Button } from "../../../../ui/controls";
import { useAttention, type AttentionEntry } from "./store";
import { clearedLine, engineName, itemTitle, whyLine } from "./text";

export interface AttentionTarget {
  threadId: string;
  terminalId: string;
}

const NOW_MS = 10_000;

function useNow(live: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!live) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), NOW_MS);
    return () => clearInterval(timer);
  }, [live]);
  return now;
}

/** The last lines, as text in a `pre`: the question (the last line) is marked. Never markup, never a link. */
function Lines({ entry }: { entry: AttentionEntry }) {
  const last = entry.excerpt.length - 1;
  return (
    <div className="ta-box">
      <div className="ta-engine">{engineName(entry.engine)}</div>
      <pre className="ta-lines">
        {entry.excerpt.map((line, index) => (
          <span key={index} className="ta-line" data-question={index === last || undefined}>
            {index === last ? "> " : ""}
            {line}
            {"\n"}
          </span>
        ))}
      </pre>
    </div>
  );
}

function Cleared({ entry }: { entry: AttentionEntry }) {
  return <p className="ta-cleared" role="status">{clearedLine(entry)}</p>;
}

/**
 * The Needs you item of a thread for engine consoles that wait for the person (docs/cli-agent-attention.md). With no client (the
 * production default) it draws nothing. Open terminal only moves focus; nothing here types into a terminal, and Answer here, Yes and No
 * are not built. One item lists every waiting terminal of the thread.
 */
export function TerminalAttention({ threadId, onOpen }: { threadId: string; onOpen(target: AttentionTarget): void }) {
  const entries = useAttention((state) => state.entries);
  const dismiss = useAttention((state) => state.dismiss);
  const mine = [...entries.values()].filter((entry) => entry.threadId === threadId && !entry.dismissed).sort((a, b) => a.toldAt - b.toldAt || a.seq - b.seq);
  const waiting = mine.filter((entry) => entry.cleared === null);
  const now = useNow(waiting.length > 0);
  if (mine.length === 0) return null;
  const done = mine.filter((entry) => entry.cleared !== null);
  const open = (entry: AttentionEntry) => onOpen({ threadId: entry.threadId, terminalId: entry.terminalId });
  return (
    <section className="ta" aria-label="Terminal agents waiting for you">
      {waiting.length === 1 && waiting[0] ? (
        <div className="ta-item" data-kind={waiting[0].kind}>
          <p className="ta-head"><span className="ta-tag">Needs you</span></p>
          <p className="ta-title">{itemTitle(waiting[0])}</p>
          <Lines entry={waiting[0]} />
          <p className="ta-why">{whyLine(waiting[0], now)}</p>
          <div className="ta-acts">
            <Button onClick={() => open(waiting[0] as AttentionEntry)}>Open terminal</Button>
            <Button tone="quiet" onClick={() => dismiss((waiting[0] as AttentionEntry).id)}>Dismiss</Button>
            <span className="ta-told">Told you at {new Date(waiting[0].toldAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
          </div>
        </div>
      ) : waiting.length > 1 ? (
        <div className="ta-item" data-many>
          <p className="ta-head"><span className="ta-tag">Needs you</span></p>
          <p className="ta-title">{waiting.length} terminals are waiting for you.</p>
          <ul className="ta-list">
            {waiting.map((entry) => (
              <li key={entry.id} className="ta-row" data-kind={entry.kind}>
                <div className="ta-who">
                  <span className="ta-name">“{entry.terminalTitle}” · {engineName(entry.engine)}</span>
                  <span className="ta-q">{entry.excerpt.at(-1) ?? ""}</span>
                </div>
                <Button aria-label={`Open terminal “${entry.terminalTitle}”`} onClick={() => open(entry)}>Open</Button>
              </li>
            ))}
          </ul>
          <p className="ta-why">Each terminal is answered on its own. Orbyn does not answer several at once.</p>
        </div>
      ) : null}
      {done.map((entry) => <Cleared key={entry.id} entry={entry} />)}
    </section>
  );
}
