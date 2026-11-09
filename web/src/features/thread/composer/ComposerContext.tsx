import { Folder, GitBranch, ShieldOff } from "lucide-react";
import type { PermissionMode, Thread } from "../../../api/types";
import { ACCESS, type useTraitsMenu } from "./ComposerTraits";

/** The checkout, branch and access row under the composer. */
export function ComposerContext({
  thread,
  hidden,
  branch,
  branchPending,
  unmanaged,
  access,
  busy,
  traits,
}: {
  thread: Thread;
  hidden: boolean;
  branch: string | null;
  branchPending: boolean;
  unmanaged: boolean;
  access: PermissionMode;
  busy: boolean;
  traits: Pick<ReturnType<typeof useTraitsMenu>, "openAt">;
}) {
  const AccessIcon = ACCESS[access].icon;
  return (
    <div className="composer-context" hidden={hidden}>
      <dl>
        <div title={thread.cwd}>
          <dt className="sr-only">Checkout</dt>
          <dd>
            <Folder size={12} aria-hidden />
            {thread.worktree ? "Worktree" : "Current checkout"}
          </dd>
        </div>
        {branch ? (
          <div>
            <dt className="sr-only">Branch</dt>
            <dd>
              <GitBranch size={12} aria-hidden />
              {branch}
            </dd>
          </div>
        ) : branchPending ? (
          <div className="composer-branch-pending" aria-hidden>
            <dd>
              <GitBranch size={12} aria-hidden />
              main
            </dd>
          </div>
        ) : null}
      </dl>
      <dl>
        <div>
          <dt className="sr-only">Access</dt>
          <dd className={unmanaged ? "tone-warn" : undefined}>
            {unmanaged ? (
              <>
                <ShieldOff size={12} aria-hidden />
                Unmanaged
              </>
            ) : (
              <button
                type="button"
                className="context-action"
                aria-haspopup="menu"
                aria-label={`Permissions: ${ACCESS[access].label}`}
                disabled={busy || !thread.lifecycle}
                onClick={(event) => traits.openAt(event.currentTarget, true)}
              >
                <AccessIcon size={12} aria-hidden />
                {ACCESS[access].label}
              </button>
            )}
          </dd>
        </div>
      </dl>
    </div>
  );
}
