import { useId, useMemo, useState } from "react";
import { navigate } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { Pill } from "../../ui/controls";
import { ActionDialog } from "./ActionDialog";
import { reposApi, useRepos } from "./api";
import type { RepoRow } from "./contract";
import { canJoinThread, defaultTicks, plain, repoState, startLabel, STATE_LABEL } from "./model";
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

/**
 * New thread in a project with several repositories: which of them the thread works in, and the branch each starts from. An
 * untrusted repository is dimmed and cannot be ticked, so nothing runs in one. Start is one click and one write.
 */
export function NewThreadDialog({ projectId, projectName, onClose }: { projectId: string; projectName: string; onClose(): void }) {
  const { view } = useRepos(projectId);
  const putThread = useWorkspace((state) => state.putThread);
  const pushToast = useUi((state) => state.pushToast);
  const [edit, setEdit] = useState<{ ticked: string[]; branches: Record<string, string> } | null>(null);
  const rows = view?.repositories ?? [];
  const limit = view?.max_per_thread ?? 8;
  const ticked = useMemo(() => edit?.ticked ?? (view ? defaultTicks(view.repositories, limit) : []), [edit, view, limit]);
  const branches = edit?.branches ?? {};
  const baseOf = (row: RepoRow) => branches[row.id] ?? row.branch ?? row.branches[0] ?? "";
  const refused = rows.filter((row) => !canJoinThread(row) && repoState(row) === "untrusted");
  const change = (next: Partial<{ ticked: string[]; branches: Record<string, string> }>) => setEdit({ ticked, branches, ...next });
  return (
    <ActionDialog title={<>New thread in {plain(projectName)} <Pill tone="info">Beta</Pill></>} confirmLabel={startLabel(ticked.length)} disabled={ticked.length === 0 || !view}
      onClose={onClose}
      onConfirm={async () => {
        const thread = await reposApi().startThread(projectId, ticked.map((repoId) => ({ repo_id: repoId, base_branch: baseOf(rows.find((row) => row.id === repoId)!) })));
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
      {refused.length > 0 ? <p className="field-hint" role="note">{refused.map((row) => plain(row.name)).join(", ")} {refused.length === 1 ? "is" : "are"} not trusted, so it cannot be included. Trust it in Settings › Projects.</p> : null}
      {ticked.length >= limit ? <p className="field-hint" role="note">A thread works in up to {limit} repositories.</p> : null}
    </ActionDialog>
  );
}
