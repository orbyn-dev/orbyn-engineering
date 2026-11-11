import { ConfirmDialog } from "../../../../ui/Dialog";
import type { ApprovalRow } from "./model";
import { GUARD_WORKERS } from "./copy";

/**
 * One dialog that names the exact command, the target and the reason, and covers that one command once.
 * It is never offered for several commands at a time, and it is not a second key.
 */
export function ApproveMyself({ row, onApprove, onClose }: { row: ApprovalRow; onApprove(): Promise<void>; onClose(): void }) {
  const guard = row.guard;
  return (
    <ConfirmDialog title="Approve this command yourself?" confirmLabel="Approve once" onConfirm={onApprove} onClose={onClose} cancelFocus>
      <p>This command deletes data in a production-linked target. You are the final authority, and your approval is one deliberate action that names the exact command and the target.</p>
      {row.block ? <pre className="ap-cmd"><code>{row.block}</code></pre> : null}
      <dl className="ap-facts">
        {guard ? <div><dt>Target</dt><dd>{guard.target} · {guard.kind} · guarded</dd></div> : null}
        <div><dt>Command</dt><dd>{guard?.short ?? row.title}</dd></div>
        <div><dt>Reason</dt><dd>{guard?.reason ?? "This command deletes data in a production-linked target."}</dd></div>
        <div><dt>Covers</dt><dd>This one command, on this target, once</dd></div>
      </dl>
      <p className="ap-muted">It is logged with the time. It isn’t a second key, and nothing else is approved with it.</p>
      <p className="ap-muted">{GUARD_WORKERS}</p>
    </ConfirmDialog>
  );
}
