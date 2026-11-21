import { CircleDot, Copy, RefreshCw, Search, X } from "lucide-react";
import type { ReactNode } from "react";
import { navigate } from "../../product/state/route";
import { Button } from "../../ui/controls";
import { TRACKER_KINDS, type TrackerKind, type TrackerRow } from "./contract";
import { trackerName } from "./model";

/** The one-block states of the list: nothing to show, no way to reach the tracker, no tracker at all. */
export function Block({ icon, title, children, actions, heading = true }: { icon: ReactNode; title: string; children?: ReactNode; actions?: ReactNode; heading?: boolean }) {
  return (
    <div className="iss-block">
      {icon}
      {heading ? <h2>{title}</h2> : <b>{title}</b>}
      {children ? <p>{children}</p> : null}
      {actions ? <div className="iss-block-actions">{actions}</div> : null}
    </div>
  );
}

export const NotReported = () => (
  <Block icon={<CircleDot size={16} aria-hidden />} title="Issues are not available">
    This service does not report issues.
  </Block>
);

export const NoIssues = ({ where, state }: { where: string; state: "open" | "closed" | "all" }) => (
  <Block icon={<CircleDot size={16} aria-hidden />} title={state === "all" ? "No issues" : `No ${state} issues`}>
    <span>{state === "all" ? `Issues from ${where} show up here.` : `${state === "open" ? "Open" : "Closed"} issues from ${where} show up here.`}</span>{" "}
    <span>Issues are created in the tracker itself.</span>
  </Block>
);

export const Unreachable = ({ who, onRetry, onCopy }: { who: string; onRetry(): void; onCopy(): void }) => (
  <Block
    icon={<X size={16} aria-hidden />}
    title={`Can’t reach ${who}`}
    actions={
      <>
        <Button tone="primary" icon={<RefreshCw size={14} aria-hidden />} onClick={onRetry}>Retry</Button>
        <Button tone="secondary" icon={<Copy size={14} aria-hidden />} onClick={onCopy}>Copy details</Button>
      </>
    }
  >
    {who === "the tracker" ? "The tracker" : who} did not answer. Your issues are not changed. Orbyn tries again when you press Retry.
  </Block>
);

export const NoMatch = ({ words, onClear }: { words: string; onClear(): void }) => (
  <div className="iss-nomatch">
    <Search size={20} aria-hidden />
    <b>No issue matches</b>
    <p>{words}</p>
    <Button icon={<X size={14} aria-hidden />} tone="primary" onClick={onClear}>Clear filters</Button>
  </div>
);

const BLURB: Record<TrackerKind, string> = {
  github: "Uses your GitHub sign-in",
  gitlab: "GitLab.com or your own server",
  azure_devops: "Work items in an organisation",
  forgejo: "Your own Forgejo or Codeberg",
  linear: "Teams and projects",
};

/**
 * No list until a tracker is connected. Connect goes to Settings, Source control, which owns the sign-in: the UI never
 * takes a token, the service keeps it in the keychain.
 */
export function NotConnected({ rows }: { rows: TrackerRow[] }) {
  const known = TRACKER_KINDS.map((tracker) => rows.find((row) => row.tracker === tracker) ?? { tracker, connected: false });
  return (
    <div className="iss-connect">
      <h2>Connect a tracker</h2>
      <p>Orbyn needs one tracker to list issues. Pick the one this project uses. Nothing is posted or changed without you.</p>
      <ul>
        {known.map((row) => (
          <li key={row.tracker}>
            <span className="iss-badge" aria-hidden>{trackerName(row.tracker).slice(0, 2).toUpperCase()}</span>
            <span className="iss-connect-name"><b>{trackerName(row.tracker)}</b><span>{BLURB[row.tracker]}</span></span>
            {row.connected ? (
              <span className="iss-quiet">Connected</span>
            ) : (
              <Button small onClick={() => navigate({ name: "settings", section: "source-control" })}>Connect</Button>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
