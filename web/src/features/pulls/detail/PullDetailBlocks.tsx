import { ChevronDown, CircleCheck, Clock, CloudOff, FileDiff, GitPullRequest, Lock, RefreshCw, SquareCheck, Trash2, TriangleAlert, X } from "lucide-react";
import type { ReactNode } from "react";
import type { PullCheck, PullFile, PullRequest } from "../../../api/types";
import { Button } from "../../../ui/controls";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { useWaited } from "../../../lib/motion";
import { PullsBanner } from "../list/PullStates";
import type { PageState } from "../model/pullModel";

/** The line above the tabs that says what state the pull request is in, with the step that fits it. */
export function PullStateBanner({
  pull,
  state,
  failed,
  own,
  reviewing,
  actions,
}: {
  pull: PullRequest;
  state: PageState;
  /** The first failing check, for the failed state. */
  failed?: PullCheck;
  /** The person opened it: a forge does not let them review it. */
  own?: boolean;
  /** The review card is open: the Request changes button stays, pressed. */
  reviewing?: boolean;
  actions: { reopen(): void; ready(): void; approve(): void; requestChanges(): void; deleteBranch(): void };
}) {
  // The person's own pull request: nobody reviews their own, and Request changes is only pressed while its card is open.
  if (own && ["open", "running", "review", "approved"].includes(state)) {
    return (
      <PullsBanner
        icon={<GitPullRequest size={14} aria-hidden />}
        actions={
          <>
            <Button small tone="quiet" disabled icon={<SquareCheck size={14} aria-hidden />}>
              Approve
            </Button>
            <Button small tone="quiet" aria-pressed={reviewing} data-pressed={reviewing || undefined} onClick={actions.requestChanges}>
              Request changes
            </Button>
          </>
        }
      >
        You opened this pull request.
      </PullsBanner>
    );
  }
  switch (state) {
    case "merged":
      return (
        <PullsBanner
          icon={<GitPullRequest size={14} aria-hidden />}
          actions={
            <Button small onClick={actions.deleteBranch} icon={<Trash2 size={14} aria-hidden />}>
              Delete branch
            </Button>
          }
        >
          Merged into {pull.base}.
        </PullsBanner>
      );
    case "closed":
      return (
        <PullsBanner
          icon={<X size={14} aria-hidden />}
          actions={
            <Button small tone="primary" icon={<RefreshCw size={14} aria-hidden />} onClick={actions.reopen}>
              Reopen
            </Button>
          }
        >
          Closed without merging.
        </PullsBanner>
      );
    case "draft":
      return (
        <PullsBanner
          icon={<GitPullRequest size={14} aria-hidden />}
          actions={
            <Button small tone="primary" icon={<CircleCheck size={14} aria-hidden />} onClick={actions.ready}>
              Ready for review
            </Button>
          }
        >
          Draft. Nobody is asked to review it yet.
        </PullsBanner>
      );
    case "conflict":
      return (
        <PullsBanner tone="warn" icon={<TriangleAlert size={14} aria-hidden />} actions={null}>
          <b>Conflicts with {pull.base}</b>
        </PullsBanner>
      );
    case "failed":
      return (
        <PullsBanner
          tone="err"
          icon={<X size={14} aria-hidden />}
          actions={
            failed?.url ? (
              <a className="btn btn-secondary btn-small" href={failed.url} target="_blank" rel="noreferrer">
                View log
              </a>
            ) : null
          }
        >
          <b>{failed ? `${failed.name} failed` : "A check failed"}</b>
          {failed?.description ? <>: {failed.description}</> : null}
        </PullsBanner>
      );
    case "changes":
      return (
        <PullsBanner tone="warn" icon={<TriangleAlert size={14} aria-hidden />} actions={null}>
          <b>Changes requested</b>
        </PullsBanner>
      );
    case "review":
      return (
        <PullsBanner
          tone="warn"
          icon={<TriangleAlert size={14} aria-hidden />}
          actions={
            <>
              <Button small tone="primary" disabled={own} icon={<SquareCheck size={14} aria-hidden />} onClick={actions.approve}>
                Approve
              </Button>
              <Button small aria-pressed={reviewing} data-pressed={reviewing || undefined} onClick={actions.requestChanges}>
                Request changes
              </Button>
            </>
          }
        >
          <b>Your review is requested</b>
        </PullsBanner>
      );
    case "approved":
      return (
        <PullsBanner icon={<SquareCheck size={14} aria-hidden />} actions={null}>
          Approved. Checks passed and there are no conflicts.
        </PullsBanner>
      );
    default:
      return null;
  }
}

/** One key and value of a card. */
export const Row = ({ name, children, tone }: { name: ReactNode; children: ReactNode; tone?: "warn" | "danger" | "blue" }) => (
  <div className="pull-kv">
    <span>{name}</span>
    <b data-tone={tone}>{children}</b>
  </div>
);

/** What the checks card says at its head, and in which colour. */
export function checksHead(pull: PullRequest): { text: string; tone?: "danger" | "blue" } {
  const counts = pull.checks;
  if (!counts) return { text: "" };
  if (counts.failing > 0) return { text: `${counts.failing} failed`, tone: "danger" };
  if (counts.running > 0) return { text: `${counts.total - counts.running} of ${counts.total} done`, tone: "blue" };
  return { text: counts.total === 0 ? "No checks" : counts.passing === 0 ? "No checks ran" : "Passed" };
}

const checkWord = (check: PullCheck) =>
  check.state === "pass" ? "Passed" : check.state === "fail" ? "Failed" : check.state === "pending" ? "running" : check.state === "skipped" ? "skipped" : check.state === "neutral" ? "neutral" : "";

export function ChecksCard({ pull, checks, collapsed }: { pull: PullRequest; checks: PullCheck[] | null; collapsed?: boolean }) {
  const head = checksHead(pull);
  if (!pull.checks && (!checks || checks.length === 0)) return null;
  const total = pull.checks?.total ?? 0;
  const running = pull.checks?.running ?? 0;
  // With the review card open there is no room for the list: one line says how they stand.
  if (collapsed)
    return (
      <div className="pull-card pull-checks-card">
        <Row name="Checks" tone={head.tone}>
          {head.text}
        </Row>
      </div>
    );
  return (
    <div className="pull-card pull-checks-card">
      <Row name="Checks" tone={head.tone}>
        {head.text}
      </Row>
      {(checks ?? []).map((check) => (
        <Row
          key={`${check.workflow ?? ""}:${check.name}`}
          tone={check.state === "fail" ? "danger" : check.state === "pending" ? "blue" : undefined}
          name={
            <span className="pull-check-name" data-state={check.state}>
              {check.state === "pass" ? (
                <SquareCheck size={14} aria-hidden />
              ) : check.state === "fail" ? (
                <X size={14} aria-hidden />
              ) : check.state === "pending" ? (
                <i className="pull-spin spin" aria-hidden />
              ) : null}
              {check.name}
            </span>
          }
        >
          {checkWord(check)}
        </Row>
      ))}
      {running > 0 && total > 0 ? (
        <div className="pull-prog" aria-hidden>
          <i style={{ width: `${Math.round(((total - running) / total) * 100)}%` }} />
        </div>
      ) : null}
    </div>
  );
}

const signed = (value: number | bigint, mark: string) => `${mark}${value}`;

/** The summary tab's list of files with their counts. */
export function FilesCard({ files, additions, deletions }: { files: PullFile[]; additions: number | null; deletions: number | null }) {
  if (files.length === 0) return null;
  return (
    <div className="pull-card pull-files-card">
      <div className="pull-kv">
        <b>Files changed</b>
        {additions !== null ? (
          <span className="pull-num">
            {signed(additions, "+")} <span className="pull-num-del">{signed(deletions ?? 0, "−")}</span>
          </span>
        ) : null}
      </div>
      <ul className="pull-filelist">
        {files.map((file) => (
          <li key={file.path}>
            <FileDiff size={14} aria-hidden />
            <span className="pull-file-name">{file.path}</span>
            <span className="pull-num">
              {signed(file.additions, "+")} <span className="pull-num-del">{signed(file.deletions, "−")}</span>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export const METHODS = [
  { value: "squash", label: "Squash and merge" },
  { value: "merge", label: "Create a merge commit" },
  { value: "rebase", label: "Rebase and merge" },
] as const;

/** The method choice with its options, beside Merge, and the line under them. */
export function MergeRow({
  method,
  onMethod,
  auto,
  onAuto,
  deleteBranch,
  onDeleteBranch,
  gate,
  menu,
  onMenu,
  onMerge,
  hidden,
  dim,
}: {
  method: string;
  onMethod(value: string): void;
  auto: boolean;
  onAuto(value: boolean): void;
  deleteBranch: boolean;
  onDeleteBranch(value: boolean): void;
  gate: { enabled: boolean; hint: string; tip?: string };
  menu: { x: number; y: number } | null;
  onMenu(at: { x: number; y: number } | null): void;
  onMerge(): void;
  /** A merged pull request has nothing to merge: only the line shows. */
  hidden: boolean;
  /** The review card is open: the merge row waits, dimmed. */
  dim?: boolean;
}) {
  const items: MenuItem[] = [
    ...METHODS.map((entry) => ({ label: entry.label, selected: method === entry.value, onSelect: () => onMethod(entry.value) })),
    { label: "Merge when checks pass", icon: <Clock size={14} aria-hidden />, selected: auto, separatorBefore: true, onSelect: () => onAuto(!auto) },
    { label: "Delete branch after merge", icon: <Trash2 size={14} aria-hidden />, selected: deleteBranch, onSelect: () => onDeleteBranch(!deleteBranch) },
  ];
  const label = METHODS.find((entry) => entry.value === method)?.label ?? METHODS[0].label;
  return (
    <>
      {hidden ? null : (
        <div className="pull-merge" data-off={!gate.enabled || undefined} data-dim={dim || undefined}>
          <button
            type="button"
            className="pull-method"
            aria-haspopup="menu"
            aria-expanded={menu !== null}
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              onMenu({ x: rect.left, y: rect.bottom + 4 });
            }}
          >
            {label}
            <ChevronDown size={14} aria-hidden />
          </button>
          <Button tone="primary" small disabled={!gate.enabled} title={gate.tip} onClick={onMerge}>
            Merge
          </Button>
        </div>
      )}
      {gate.hint ? <p className="pull-merge-hint">{gate.hint}</p> : null}
      <Presence>{menu ? <Menu x={menu.x} y={menu.y} label="Merge method" heading="Merge method" items={items} onClose={() => onMenu(null)} /> : null}</Presence>
    </>
  );
}

/** What stands in for the page while it loads, drawn only once the wait passes 400 ms. */
export function PullDetailBlocksSkeleton() {
  const shown = useWaited(true);
  // Line widths as the pr-detail-loading frame draws them.
  const card = (widths: number[]) => (
    <div className="pull-card pull-skel-card">
      {widths.map((width, at) => <i key={at} style={{ width: `${width}%` }} />)}
    </div>
  );
  return (
    <div className="pull-page-skel" role="status" aria-label="Loading pull request">
      {shown ? (
        <div className="pull-cols" aria-hidden>
          <div className="pull-main">
            {card([40, 100, 82])}
            {card([30, 90, 64])}
          </div>
          <div className="pull-side">{card([100, 100, 100, 100])}</div>
        </div>
      ) : null}
    </div>
  );
}

/** The page's one-block states: the forge did not answer, or the sign-in ended. */
export function PullBlock({ icon, title, children, actions }: { icon: ReactNode; title: string; children: ReactNode; actions: ReactNode }) {
  return (
    <div className="pulls-block pull-block">
      {icon}
      <h2>{title}</h2>
      <p>{children}</p>
      <div className="pulls-block-actions">{actions}</div>
    </div>
  );
}

/** Nothing cached to show: the page says it could not load, and offers to try again. */
export const UnreachableBlock = ({ onRetry }: { onRetry(): void }) => (
  <div className="pulls-block pull-block">
    <X size={16} aria-hidden />
    <p>Couldn{"\u2019"}t load details.</p>
    <div className="pulls-block-actions">
      <Button tone="primary" icon={<RefreshCw size={14} aria-hidden />} onClick={onRetry}>
        Try again
      </Button>
    </div>
  </div>
);

export const SignedOutBlock = ({ forge, number, onSignIn, onLater }: { forge: string; number: number; onSignIn(): void; onLater(): void }) => (
  <PullBlock
    icon={<Lock size={16} aria-hidden />}
    title={`Sign in to ${forge}`}
    actions={
      <>
        <Button tone="primary" onClick={onSignIn}>
          Sign in to {forge}
        </Button>
        <Button onClick={onLater}>Not now</Button>
      </>
    }
  >
    Your {forge} sign-in ended, so Orbyn can{"’"}t load pull request #{number}. Nothing is posted without you.
  </PullBlock>
);

export const OfflineLine = ({ ago, onRetry }: { ago: string; onRetry(): void }) => (
  <PullsBanner
    live
    icon={<CloudOff size={14} aria-hidden />}
    actions={
      <Button small icon={<RefreshCw size={14} aria-hidden />} onClick={onRetry}>
        Retry
      </Button>
    }
  >
    You{"’"}re offline. Showing what was loaded {ago}.
  </PullsBanner>
);
