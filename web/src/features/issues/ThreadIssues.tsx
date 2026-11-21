import { CircleDot } from "lucide-react";
import { useRef, useState, type ReactNode } from "react";
import type { Thread } from "../../api/types";
import { useUi } from "../../product/state/ui";
import { DRow } from "../../product/ui/DetailRows";
import { Button } from "../../ui/controls";
import { useTrackers, issuesApi } from "./api";
import type { IssueRef } from "./contract";
import { IssueGlyph } from "./IssueGlyph";
import { LinkIssueDialog } from "./LinkIssueDialog";
import { useThreadIssues } from "./links";
import { reasonOf, refKey, stateWord, trackerName } from "./model";
import "./issues.css";

function IssueLine({ thread, issue, first, icon, onOpen, onUnlinked }: { thread: Thread; issue: IssueRef; first: boolean; icon: ReactNode; onOpen(): void; onUnlinked(): Promise<void> }) {
  const pushToast = useUi((state) => state.pushToast);
  const [busy, setBusy] = useState(false);
  // One click, one write: a second press while the first is in flight is not a second write, even before a render.
  const pending = useRef(false);
  const unlink = async () => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    try {
      await issuesApi().unlink(thread.id, issue);
      await onUnlinked();
    } catch (failure) {
      pending.current = false;
      setBusy(false);
      pushToast(`Couldn’t unlink it. Nothing changed. ${reasonOf(failure)}`);
    }
  };
  return (
    <DRow
      icon={first ? icon : <span className="iss-dgap" aria-hidden />}
      label={first ? "Issues" : ""}
      ariaLabel={`${issue.display_key} ${issue.title}`}
      title={issue.title}
      onClick={onOpen}
      value={<><span className="t2">{trackerName(issue.tracker)} · {issue.display_key}</span><span className="pillx iss-dstate"><IssueGlyph state={issue.state} size={12} />{stateWord(issue.state)}</span></>}
      sub={<span className="iss-dtitle" title={issue.title}>{issue.title}</span>}
      extra={<Button small tone="quiet" aria-label={`Unlink ${issue.display_key}`} disabled={busy} onClick={() => void unlink()}>Unlink</Button>}
    />
  );
}

/**
 * The Details card's Issues row, under Pull requests: one line per linked issue with its state as a shape and a word, the key, the
 * title and the tracker, and Unlink on the line (it removes the link from this thread only and leaves the issue alone). Pressing a
 * line opens the issue beside the thread. A thread with none says so and offers Link an issue…. Nothing is drawn while the service
 * reports no trackers and this thread has no link.
 */
export function IssuesRow({ thread, onOpened }: { thread: Thread; onOpened?(): void }) {
  const { issues, reload } = useThreadIssues(thread.id);
  const { rows } = useTrackers();
  const openFileTab = useUi((state) => state.openFileTab);
  const [linking, setLinking] = useState(false);
  if (rows === null && issues.length === 0) return null;
  const icon = <CircleDot size={16} aria-hidden />;
  const link = <button type="button" className="dlink" aria-haspopup="dialog" onClick={() => setLinking(true)}>Link an issue…</button>;
  return (
    <>
      {issues.length === 0 ? (
        <DRow icon={icon} label="Issues" value={<span className="t3">None linked</span>} sub={<span className="dlinks">{link}</span>} />
      ) : (
        <>
          {issues.map((issue, at) => (
            <IssueLine
              key={refKey(issue)}
              thread={thread}
              issue={issue}
              first={at === 0}
              icon={icon}
              onUnlinked={reload}
              onOpen={() => { openFileTab({ kind: "issue", tracker: issue.tracker, scope: issue.scope, key: issue.key }); onOpened?.(); }}
            />
          ))}
          {issues.length > 1 ? <div className="iss-dlinkrow"><span className="dlinks">{link}</span></div> : null}
        </>
      )}
      {linking ? <LinkIssueDialog thread={thread} onClose={() => setLinking(false)} /> : null}
    </>
  );
}
