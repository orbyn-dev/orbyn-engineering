import { ApiFailure } from "../../api/client";
import { useId, useMemo, useState, type ReactNode } from "react";
import { navigate } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { Field, Pill } from "../../ui/controls";
import { ActionDialog } from "./ActionDialog";
import { reposApi, useRepos } from "./api";
import type { RepoRow, StartThreadInput } from "./contract";
import { canJoinThread, defaultTicks, plain, repoState, startLabel, STATE_LABEL } from "./model";
import { checkBranchName } from "./scanModel";
import "./repos.css";

function Choice({ row, ticked, branch, disabled, limitReached, onTick, onBranch }: {
  row: RepoRow; ticked: boolean; branch: string; disabled: boolean; limitReached: boolean; onTick(on: boolean): void; onBranch(branch: string): void;
}) {
  const id = useId();
  const joinable = canJoinThread(row);
  const state = repoState(row);
  return (
    <li className="repos-pick" data-dim={!joinable || undefined}>
      <input id={id} type="checkbox" checked={ticked} disabled={disabled || !joinable || (!ticked && limitReached)} onChange={(event) => onTick(event.target.checked)} />
      <label htmlFor={id} className="repos-pick-text">
        <span className="repos-name">{plain(row.name)}</span>
        <code className="repos-path">{plain(row.path)}</code>
      </label>
      <select aria-label={`Branch in ${plain(row.name)}`} className="repos-branch-pick" value={branch} disabled={disabled || !joinable || row.branches.length === 0}
        onChange={(event) => onBranch(event.target.value)}>
        {(row.branches.length === 0 ? [row.branch ?? ""] : row.branches).map((name) => <option key={name} value={name}>{plain(name)}</option>)}
      </select>
      <Pill tone={state === "trusted" ? "accent" : "warn"}>{STATE_LABEL[state]}</Pill>
    </li>
  );
}

/** One labelled row of the dialog, the label beside the control the host puts in it. */
const Row = ({ label, children }: { label: string; children: ReactNode }) => (
  <div className="repos-start-row"><span className="repos-start-label">{label}</span><span className="repos-start-control">{children}</span></div>
);

/**
 * New thread in a project with several repositories: which of them the thread works in, the branch each starts from, and the
 * engine and permission it starts with. An untrusted repository is dimmed and cannot be ticked, so nothing runs in one. Start is
 * one click and one write. A permission the engine does not offer is refused by the service (`permission_mode_unsupported`, 400)
 * before anything is made: the reason shows and the dialog stays open. An untrusted repository is listed but cannot be ticked.
 *
 * The engine and permission pickers are the new-thread flow's own: the host that already owns them hands them in as `engineControl`
 * and `permissionControl`, with the choice they edit as `start`, so this feature does not import the thread feature (which imports
 * this one). `note` is the one sentence that says which permission modes the engine does not offer.
 */
export function NewThreadDialog({ projectId, projectName, start, engineControl, permissionControl, note, onClose }: {
  projectId: string;
  projectName: string;
  /** The engine, model and permission the thread starts with, as the host's pickers hold them right now. */
  start: Pick<StartThreadInput, "engine" | "model" | "permission_mode">;
  engineControl: ReactNode;
  permissionControl: ReactNode;
  note?: string | null;
  onClose(): void;
}) {
  const { view } = useRepos(projectId);
  const putThread = useWorkspace((state) => state.putThread);
  const pushToast = useUi((state) => state.pushToast);
  const [edit, setEdit] = useState<{ ticked: string[]; branches: Record<string, string> } | null>(null);
  const rows = view?.repositories ?? [];
  const limit = view?.max_per_thread ?? 8;
  const ticked = useMemo(() => edit?.ticked ?? (view ? defaultTicks(view.repositories, limit) : []), [edit, view, limit]);
  const branches = edit?.branches ?? {};
  const baseOf = (row: RepoRow) => branches[row.id] ?? row.branch ?? row.branches[0] ?? "";
  const [branchText, setBranchText] = useState("");
  // A task folder (2 or more repositories) shares one branch name. Left empty, the service makes one name.
  const taskFolder = ticked.length >= 2;
  const branchCheck = branchText.trim() === "" ? null : checkBranchName(branchText);
  const branchError = taskFolder && branchCheck && !branchCheck.ok ? branchCheck.reason : undefined;
  const branchBad = branchError !== undefined;
  const refused = rows.filter((row) => !canJoinThread(row) && repoState(row) === "untrusted");
  const change = (next: Partial<{ ticked: string[]; branches: Record<string, string> }>) => setEdit({ ticked, branches, ...next });
  return (
    <ActionDialog title={<>New thread in {plain(projectName)} <Pill tone="info">Beta</Pill></>} confirmLabel={startLabel(ticked.length)} disabled={ticked.length === 0 || !view || branchBad}
      onClose={onClose}
      onConfirm={async () => {
        const thread = await reposApi().startThread(projectId, { repos: ticked.map((repoId) => ({ repo_id: repoId, base_branch: baseOf(rows.find((row) => row.id === repoId)!) })), ...start, ...(taskFolder && branchCheck?.ok ? { branch_name: branchCheck.name } : {}) })
          .catch((failure: unknown) => {
            // The engine does not offer this permission mode: say so, and the dialog stays open for another choice.
            if (failure instanceof ApiFailure && failure.code === "permission_mode_unsupported") throw new ApiFailure(failure.code, `This engine does not offer that permission. ${failure.message} Pick another permission and start again.`, failure.status);
            throw failure;
          });
        putThread(thread);
        pushToast(`Started a thread in ${ticked.length === 1 ? "1 repository" : `${ticked.length} repositories`}`);
        navigate({ name: "thread", id: thread.id });
      }}>
      <p className="field-hint">Pick the repositories this thread works in. Each one gets its own branch for the thread.</p>
      {view ? (
        <ul className="repos-picks" aria-label="Repositories">
          {rows.map((row) => (
            <Choice key={row.id} row={row} ticked={ticked.includes(row.id)} branch={baseOf(row)} disabled={false} limitReached={ticked.length >= limit}
              onTick={(on) => change({ ticked: on ? [...ticked, row.id] : ticked.filter((entry) => entry !== row.id) })}
              onBranch={(name) => change({ branches: { ...branches, [row.id]: name } })} />
          ))}
        </ul>
      ) : <p className="field-hint">Loading repositories…</p>}
      {taskFolder ? (
        <Field label="Branch name" hint="Optional. One new branch with this name is made in every chosen repository. Leave it empty and Orbyn picks one name for all." error={branchError}>
          {(field) => <input {...field} className="dialog-field mono" value={branchText} placeholder="feat/login-fix" spellCheck={false} maxLength={200} onChange={(event) => setBranchText(event.target.value)} />}
        </Field>
      ) : null}
      <Row label="Engine">{engineControl}</Row>
      <Row label="Permission">{permissionControl}</Row>
      {note ? <p className="field-hint" role="note">{note}</p> : null}
      {refused.length > 0 ? <p className="field-hint" role="note">{refused.map((row) => plain(row.name)).join(", ")} {refused.length === 1 ? "is" : "are"} not trusted, so it cannot be included. Trust it in Settings › Projects.</p> : null}
      {ticked.length >= limit ? <p className="field-hint" role="note">A thread works in up to {limit} repositories.</p> : null}
    </ActionDialog>
  );
}
