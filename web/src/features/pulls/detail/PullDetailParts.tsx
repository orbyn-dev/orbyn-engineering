import type { ReactNode } from "react";
import type { PullRequestDetail } from "../../../api/types";
import type { ApiFailure } from "../../../api/client";
import { pullsApi } from "../../../api/pulls";
import type { useConnectionKind } from "../../../state/connection";
import { Button } from "../../../ui/controls";
import type { MenuItem } from "../../../ui/Menu";
import { ErrorState, Loading } from "../../../ui/states";
import { baseChangeAllowed } from "../model/pullModel";
import { PullPageSkeleton, SignedOutBlock, UnreachableBlock } from "./PullPage";
import { navigate } from "../../../state/route";

export type Tab = "summary" | "guide" | "timeline" | "code" | "checks";

export const TAB_LABELS: Record<Tab, string> = { summary: "Summary", guide: "Guide", timeline: "Timeline", code: "Code", checks: "Checks" };

export const PAGE_LABELS: Record<Tab, string> = { summary: "Summary", guide: "Guide", timeline: "Conversation", code: "Files changed", checks: "Checks" };

/** The rows of the pull request's More actions menu. */
export function pullMenuItems({
  page,
  pull,
  current,
  open,
  forge,
  connection,
  setEditing,
  setConfirm,
  setChangingBase,
  work,
  update,
  act,
  checkout,
}: {
  page: boolean;
  pull: PullRequestDetail["pull"];
  current: PullRequestDetail;
  open: boolean;
  forge: string;
  connection: ReturnType<typeof useConnectionKind>;
  setEditing(title: string | null): void;
  setConfirm(confirm: "close" | "delete_branch" | null): void;
  setChangingBase(changing: boolean): void;
  work(): Promise<void>;
  update(change: Parameters<typeof pullsApi.update>[2], done: string): Promise<void>;
  act(action: string, done: string): Promise<void>;
  checkout(worktree: boolean): Promise<void>;
}): MenuItem[] {
  return [
    ...(page
      ? [
          { label: "Edit title", onSelect: () => setEditing(pull.title) },
          { label: "Work on this", onSelect: () => void work() },
          { label: `Open on ${forge === "The host" ? "the forge" : forge}`, disabled: connection === "offline", onSelect: () => void window.open(pull.url, "_blank", "noreferrer") },
          ...(open ? [{ label: "Close pull request", destructive: true, onSelect: () => setConfirm("close") }] : []),
        ]
      : []),
    ...(open
      ? [
          pull.draft
            ? { label: "Ready for review", onSelect: () => void update({ draft: false }, "Ready for review") }
            : { label: "Convert to draft", onSelect: () => void update({ draft: true }, "Converted to draft") },
          ...(baseChangeAllowed(current) ? [{ label: "Change base branch", onSelect: () => setChangingBase(true) }] : []),
          { label: "Update branch", onSelect: () => void act("update_branch", "Branch updated") },
          { label: "Update with rebase", onSelect: () => void act("rebase", "Branch rebased") },
          {
            label: current.maintainer_can_modify ? "Stop maintainer edits" : "Allow maintainer edits",
            onSelect: () =>
              void update({ maintainer_can_modify: !current.maintainer_can_modify }, "Maintainer edits changed"),
          },
        ]
      : []),
    { label: "Check out here", separatorBefore: true, onSelect: () => void checkout(false) },
    { label: "Check out in a worktree", onSelect: () => void checkout(true) },
    ...(pull.state === "merged"
      ? [{ label: "Revert changes", onSelect: () => void act("revert", "Revert opened") }]
      : []),
    {
      label: "Delete branch",
      destructive: true,
      separatorBefore: true,
      onSelect: () => setConfirm("delete_branch"),
    },
  ];
}

/** The inline form that renames the pull request. */
export function TitleForm({
  editing,
  setEditing,
  update,
}: {
  editing: string | null;
  setEditing(title: string | null): void;
  update(change: Parameters<typeof pullsApi.update>[2], done: string): Promise<void>;
}) {
  return (
    <form
      className="pull-title-form"
      onSubmit={(event) => {
        event.preventDefault();
        void update({ title: editing ?? "" }, "Title saved").then(() => setEditing(null));
      }}
    >
      <input aria-label="Title" value={editing ?? ""} autoFocus onChange={(event) => setEditing(event.target.value)} />
      <Button small tone="primary" type="submit" disabled={!(editing ?? "").trim()}>
        Save
      </Button>
      <Button small onClick={() => setEditing(null)}>
        Cancel
      </Button>
    </form>
  );
}

type PageHead = (title: ReactNode, pill?: ReactNode) => ReactNode;

/** The pull request while its first read runs: the page's skeleton, or a plain loading row in the panel. */
export function PullDetailLoading({ page, number, pageHead }: { page: boolean; number: number; pageHead: PageHead }) {
  if (page)
    return (
      <article className="pull-page" aria-label={`Pull request #${number}`}>
        {pageHead(<i className="pull-title-skel" aria-hidden />)}
        <div className="pull-page-tabs" aria-hidden>
          {(["summary", "guide", "timeline", "code", "checks"] as const).map((entry) => (
            <button key={entry} type="button" tabIndex={-1} disabled>
              {PAGE_LABELS[entry]}
            </button>
          ))}
        </div>
        <PullPageSkeleton />
      </article>
    );
  return (
    <Loading label="Loading pull request" rows={5} />
  );
}

/** The pull request when its first read failed: sign-in or retry on the page, an error row in the panel. */
export function PullDetailError({
  page,
  number,
  pageHead,
  signedOut,
  forgeLabel,
  failure,
  onRetry,
}: {
  page: boolean;
  number: number;
  pageHead: PageHead;
  signedOut: boolean;
  forgeLabel: string;
  failure: ApiFailure;
  onRetry(): void;
}) {
  if (page) {
    return (
      <article className="pull-page" aria-label={`Pull request #${number}`}>
        {pageHead(
          <h1 tabIndex={-1} data-page-title>
            Pull request #{number}
          </h1>,
        )}
        {signedOut ? (
          <SignedOutBlock forge={forgeLabel} number={number} onSignIn={() => navigate({ name: "settings", section: "source-control" })} onLater={() => (window.history.length > 1 ? window.history.back() : navigate({ name: "pulls" }))} />
        ) : (
          <UnreachableBlock onRetry={onRetry} />
        )}
      </article>
    );
  }
  return (
    <ErrorState title="Couldn't load this pull request" failure={failure} onRetry={onRetry} />
  );
}
