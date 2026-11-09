import { MessageSquare, SquareCheck, Users, X } from "lucide-react";
import { useId, useRef, useState, type ReactNode } from "react";
import type { PullListEntry } from "../../../api/types";
import { deriveProjectIdentity } from "../../../product/projects/projectIdentity";
import { forgeName } from "../../../product/pulls/labels";
import { checksView, requestedName, yourReview, type PullDetails } from "../model/pullModel";
import type { DetailsStatus } from "../model/usePullDetails";

/** What the cells need to know about the details request, and who is signed in. */
export interface CellEnv {
  status: DetailsStatus;
  /** True once the request has run 400 ms, which is when the pending look appears. */
  waited: boolean;
  paused: readonly string[];
  viewers: readonly string[];
}

/** A tooltip under its cell: on hover or keyboard focus, and Escape puts it away. */
function Cell({ className, tip, children }: { className: string; tip?: ReactNode; children: ReactNode }) {
  const id = useId();
  const [away, setAway] = useState(false);
  return (
    <span
      className={`pull-cell ${className}`}
      tabIndex={tip ? 0 : undefined}
      aria-describedby={tip ? id : undefined}
      data-away={away || undefined}
      onKeyDown={(event) => {
        if (event.key === "Escape" && tip) {
          event.stopPropagation();
          setAway(true);
        }
      }}
      onMouseLeave={() => setAway(false)}
      onBlur={() => setAway(false)}
    >
      {children}
      {tip ? (
        <span role="tooltip" id={id} className="pull-tip">
          {tip}
        </span>
      ) : null}
    </span>
  );
}

const NOUN = { checks: "check counts", reviewers: "reviewers", review: "your review", comments: "comment counts" } as const;
type Field = keyof typeof NOUN;
const PAUSED_FIELD: Record<Field, string | null> = { checks: null, reviewers: "requested_reviewers", review: "viewer_review_state", comments: "comment_count" };

/**
 * One details-backed cell. A value shows as soon as it exists. Without one the cell keeps
 * its final size and says why: still loading (nothing for 400 ms, then a quiet bar),
 * could not load, paused by the rate limit, or not something the forge provides.
 */
function Slot({
  field,
  width,
  circles,
  env,
  pull,
  value,
  className,
  tip,
}: {
  field: Field;
  width: number;
  circles?: boolean;
  env: CellEnv;
  pull: PullListEntry["pull"];
  /** `undefined` while unknown; `null` for a known empty cell. */
  value: ReactNode | null | undefined;
  className: string;
  tip?: ReactNode;
}) {
  const pending = value === undefined && (env.status === "loading" || env.status === "idle");
  const wasPending = useRef(false);
  if (pending) wasPending.current = true;
  const size = { width, height: 20 };
  if (value !== undefined) {
    return (
      <Cell className={className} tip={tip}>
        <span className={wasPending.current ? "pull-slot pull-reveal" : "pull-slot"} style={{ minWidth: width, height: 20 }}>
          {value}
        </span>
      </Cell>
    );
  }
  if (pending) {
    return (
      <Cell className={className}>
        <span className="pull-slot" style={size} aria-hidden>
          {env.waited ? (
            circles ? (
              <span className="pull-skel-circles">
                <i />
                <i />
              </span>
            ) : (
              <i className="pull-pending" style={{ width }} />
            )
          ) : null}
        </span>
      </Cell>
    );
  }
  const forge = forgeName(pull.url);
  const paused = (() => {
    const named = PAUSED_FIELD[field];
    return env.status === "ready" && named !== null && env.paused.includes(named);
  })();
  // Check counts come with the list, so a details failure says nothing about them.
  const why =
    env.status === "failed" && field !== "checks"
      ? "Couldn\u2019t load details"
      : paused
        ? field === "comments"
          ? "Comment counts are paused until GitHub\u2019s rate limit resets"
          : `${NOUN[field][0]!.toUpperCase()}${NOUN[field].slice(1)} are paused until GitHub\u2019s rate limit resets`
        : `${forge} doesn\u2019t provide ${NOUN[field]} here.`;
  return (
    <Cell className={className} tip={why}>
      <span className="pull-slot" style={size}>
        <span className="pull-dash">{"—"}</span>
      </span>
    </Cell>
  );
}

export function ChecksCell({ entry, env }: { entry: PullListEntry; env: CellEnv }) {
  const { pull } = entry;
  const view = checksView(pull);
  const value =
    view === undefined || view === null ? undefined : (
      <span className="pull-res" data-tone={view.tone}>
        {view.tone === "fail" ? <X size={14} aria-hidden /> : null}
        {view.tone === "running" ? <i className="pull-spin spin" aria-hidden /> : null}
        {view.tone === "pass" ? <SquareCheck size={14} aria-hidden /> : null}
        <span className="pull-res-word">
          <span className="sr-only">Checks </span>
          {view.label}
        </span>
      </span>
    );
  const tip = view?.tip ? (
    <>
      <b>{view.tip.title}</b>
      {view.tip.lines.map((line) => (
        <span key={line} className="pull-tip-line">
          {line}
        </span>
      ))}
    </>
  ) : undefined;
  return <Slot field="checks" width={72} env={env} pull={pull} value={value} className="pull-checks-cell" tip={tip} />;
}

function Avatar({ name, team }: { name: string; team: boolean }) {
  if (team)
    return (
      <span className="pull-av pull-av-team" aria-hidden>
        <Users size={12} />
      </span>
    );
  const identity = deriveProjectIdentity(name);
  return (
    <span className={`pull-av project-color-${identity.color}`} aria-hidden>
      {identity.monogram}
    </span>
  );
}

export function ReviewersCell({ entry, details, env }: { entry: PullListEntry; details: PullDetails | undefined; env: CellEnv }) {
  const people = details?.requested_reviewers;
  const value =
    people === undefined ? undefined : people.length === 0 ? null : (
      <>
        <span className="pull-avs">
          {people.slice(0, 2).map((one) => (
            <Avatar key={`${one.kind}:${one.name}`} name={one.name} team={one.kind === "team"} />
          ))}
        </span>
        {people.length > 2 ? <span className="pull-more">+{people.length - 2}</span> : null}
      </>
    );
  const names = people?.map((one) => (one.kind === "team" ? requestedName(`team:${one.name}`) : one.name)).join(", ");
  return (
    <Slot
      field="reviewers"
      width={44}
      circles
      env={env}
      pull={entry.pull}
      value={value}
      className="pull-reviewers-cell"
      tip={people && people.length > 0 ? names : undefined}
    />
  );
}

export function YourReviewCell({ entry, details, env }: { entry: PullListEntry; details: PullDetails | undefined; env: CellEnv }) {
  const mine = details ? yourReview(details, env.viewers) : undefined;
  const value =
    mine === undefined ? undefined : mine === null ? null : (
      <span className="pull-mine" data-tone={mine.tone}>
        {mine.tone === "approved" ? <SquareCheck size={14} aria-hidden /> : null}
        {mine.label}
      </span>
    );
  return <Slot field="review" width={84} env={env} pull={entry.pull} value={value} className="pull-yours-cell" />;
}

export function CommentsCell({ entry, details, env }: { entry: PullListEntry; details: PullDetails | undefined; env: CellEnv }) {
  const count = details?.comment_count;
  const value =
    count === undefined ? undefined : count === 0 ? null : (
      <span className="pull-count">
        <MessageSquare size={14} aria-hidden />
        <span aria-hidden>{count}</span>
        <span className="sr-only">{`${count} ${count === 1 ? "comment" : "comments"}, including review comments`}</span>
      </span>
    );
  return (
    <Slot
      field="comments"
      width={28}
      env={env}
      pull={entry.pull}
      value={value}
      className="pull-comments-cell"
      tip={count ? `${count} ${count === 1 ? "comment" : "comments"}, including review comments` : undefined}
    />
  );
}
