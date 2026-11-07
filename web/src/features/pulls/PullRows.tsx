import { CircleCheck, CircleDot, CircleX, GitMerge, GitPullRequest, MessageSquare } from "lucide-react";
import type { PullListEntry } from "../../api/types";
import { compactAge } from "../../lib/format";
import { IconButton } from "../../ui/controls";
import { ChecksCell, CommentsCell, ReviewersCell, YourReviewCell, type CellEnv } from "./PullCells";
import { detailsKey, type DetailsByKey } from "./pullModel";

/** The table's column titles. The comments column is named by its icon. */
export function PullsHead() {
  return (
    <div className="pulls-head" aria-hidden>
      <span />
      <span>Pull request</span>
      <span>Checks</span>
      <span>Reviewers</span>
      <span>Your review</span>
      <span>
        <MessageSquare size={14} aria-hidden />
      </span>
      <span>Updated</span>
    </div>
  );
}

/** Six static rows at the real row height, for a list or a filter that is still waiting. */
export function PullsSkeleton() {
  return (
    <ul className="pull-rows pull-skel" aria-hidden>
      {Array.from({ length: 6 }, (_, at) => (
        <li key={at}>
          <i className="pull-skel-icon" />
          <span className="pull-skel-title">
            <i style={{ width: "55%" }} />
            <i style={{ width: "30%" }} />
          </span>
          <i className="pull-skel-checks" />
          <span />
          <i className="pull-skel-review" />
          <span />
          <i className="pull-skel-updated" />
        </li>
      ))}
    </ul>
  );
}

export interface PullRowsProps {
  rows: readonly PullListEntry[];
  details: DetailsByKey;
  env: CellEnv;
  picked: readonly PullListEntry[];
  revealed: boolean;
  projectName(id: string): string;
  onPick(entry: PullListEntry): void;
  onOpen(entry: PullListEntry): void;
  onReopen(entry: PullListEntry): void;
  onReady(entry: PullListEntry): void;
  onMerge(entry: PullListEntry): void;
  onClose(entry: PullListEntry): void;
}

export function PullRows(props: PullRowsProps) {
  const { rows, details, env, picked } = props;
  return (
    <ul className="pull-rows" data-reveal={props.revealed || undefined}>
      {rows.map((entry) => {
        const { project_id: project, pull } = entry;
        const isPicked = picked.some((one) => one.project_id === project && one.pull.number === pull.number);
        const row = details.get(detailsKey(project, pull.number));
        return (
          <li
            key={`${project}:${pull.number}`}
            onClick={(event) => {
              if (!(event.target as HTMLElement).closest("button, input, a, .pull-cell[tabindex]")) props.onOpen(entry);
            }}
          >
            <span className="pull-lead" data-picked={isPicked || undefined}>
              <GitPullRequest size={14} className="pull-lead-icon" aria-hidden />
              <input
                type="checkbox"
                className="pull-row-pick"
                aria-label={`Select #${pull.number}`}
                checked={isPicked}
                onChange={() => props.onPick(entry)}
              />
            </span>
            <button type="button" className="pull-open" onClick={() => props.onOpen(entry)}>
              <span className="pull-row-name" data-quiet={pull.draft || pull.state === "closed" || undefined}>
                {pull.draft ? "Draft · " : ""}
                {pull.title}
              </span>
              <small>{[`#${pull.number}`, pull.head, props.projectName(project), pull.author].filter(Boolean).join(" · ")}</small>
            </button>
            <ChecksCell entry={entry} env={env} />
            <ReviewersCell entry={entry} details={row} env={env} />
            <YourReviewCell entry={entry} details={row} env={env} />
            <CommentsCell entry={entry} details={row} env={env} />
            <span className="pull-age">{pull.updated_at ? compactAge(pull.updated_at) : ""}</span>
            <span className="pull-row-actions">
              {pull.state === "closed" ? (
                <IconButton label={`Reopen #${pull.number}`} onClick={() => props.onReopen(entry)}>
                  <CircleDot size={14} aria-hidden />
                </IconButton>
              ) : null}
              {pull.state === "open" && pull.draft ? (
                <IconButton label={`Ready for review #${pull.number}`} onClick={() => props.onReady(entry)}>
                  <CircleCheck size={14} aria-hidden />
                </IconButton>
              ) : null}
              {pull.state === "open" ? (
                <>
                  <IconButton label={`Merge #${pull.number}`} onClick={() => props.onMerge(entry)}>
                    <GitMerge size={14} aria-hidden />
                  </IconButton>
                  <IconButton label={`Close #${pull.number}`} onClick={() => props.onClose(entry)}>
                    <CircleX size={14} aria-hidden />
                  </IconButton>
                </>
              ) : null}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
