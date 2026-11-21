import { ConfirmDialog } from "../../ui/Dialog";
import { plain, runGrantText, type RunConfirmation } from "./model";

/**
 * The confirm sheet of a workflow run the service answered `409 confirm_required` (docs/execution-profiles-and-library.md, "Grants a run
 * needs"). It is the same `ConfirmDialog` the profile apply uses. It lists the grants in plain words and the env set names; a value is
 * never in the answer and never here. Run anyway hands back the grants it showed, exactly, once per click (the dialog's own busy guard
 * disables the button for the click; the caller guards the request). Cancel sends nothing. Nothing is remembered: the next run asks the
 * service again, since an edit to the command drops the service's own record.
 */
export function RunConfirm({ name, confirmation, onRun, onClose }: { name: string; confirmation: RunConfirmation; onRun(grants: string[]): void; onClose(): void }) {
  return (
    <ConfirmDialog
      title={`Run “${plain(name, 40)}”?`}
      confirmLabel="Run anyway"
      tone="danger"
      cancelFocus
      keepOpen
      onClose={onClose}
      onConfirm={async () => { onRun([...confirmation.grants]); }}
    >
      <p>This run needs your confirmation:</p>
      <ul className="lib-grants">{confirmation.grants.map((code) => <li key={code}>{runGrantText(code)}</li>)}</ul>
      {confirmation.envSets.length > 0 ? (
        <>
          <p>Env sets applied:</p>
          <ul className="lib-grants" aria-label="Env sets applied">{confirmation.envSets.map((set) => <li key={set}><code>{plain(set, 60)}</code></li>)}</ul>
        </>
      ) : null}
      <p className="field-hint">You are asked again whenever the service needs it.</p>
    </ConfirmDialog>
  );
}
