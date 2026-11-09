import { useEffect, useState } from "react";
import { api } from "../../../api/client";
import type { Thread } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { useVisibleProjects } from "../../../state/workspace";
import { Button } from "../../../ui/controls";
import { Empty, Loading } from "../../../ui/states";
import { unarchiveThread } from "../../workspace";
import { ConfirmDialog } from "../../../ui/Dialog";
import { heading } from "./parts";

export function ArchiveGroupHead({ name, removed }: { name: string; removed: boolean }) {
  return <p className="set-grp set-grp-tight">{name}{removed ? <span className="set-pill">Removed</span> : null}</p>;
}

export function ArchiveSection() {
  const projects = useVisibleProjects();
  const [threads, setThreads] = useState<Thread[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<Thread | null>(null);
  const columns = { gridTemplateColumns: "minmax(0, 1fr) 110px 190px" };
  useEffect(() => {
    const abort = new AbortController();
    api.archivedThreads(abort.signal).then(
      (list) => setThreads(list.sort((a, b) => b.updated_at.localeCompare(a.updated_at))),
      (failure: unknown) => {
        if (!abort.signal.aborted) setError(toFailure(failure).message);
      },
    );
    return () => abort.abort();
  }, []);
  const remove = async (thread: Thread) => {
    setThreads((threads ?? []).filter((entry) => entry.id !== thread.id));
    await api.deleteThread(thread.id).catch((failure: unknown) => setError(toFailure(failure).message));
  };
  // The sidebar's order first, then the projects that were removed, each under the name it had.
  const groups = threads === null || !projects ? [] : [
    ...projects.map((project) => ({ key: project.id, name: project.name, removed: false, owned: threads.filter((thread) => !thread.project_removed && thread.project_id === project.id) })),
    ...[...new Set(threads.filter((thread) => thread.project_removed).map((thread) => thread.project_id))].map((id) => {
      const owned = threads.filter((thread) => thread.project_removed && thread.project_id === id);
      return { key: id, name: owned[0]?.removed_project_name ?? id, removed: true, owned };
    }),
  ].filter((group) => group.owned.length > 0);
  return (
    <>
      {heading("Archive")}
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
      {!threads || !projects ? (
        <Loading label="Loading archived threads" rows={3} />
      ) : groups.length === 0 ? (
        <Empty title="No archived threads" />
      ) : (
        <>
          {groups.map((group) => (
              <section key={group.key} aria-label={group.name}>
                <ArchiveGroupHead name={group.name} removed={group.removed} />
                <div className="set-tbl" role="table" aria-label={`Archived threads in ${group.name}`}>
                  {group.owned.map((thread) => (
                    <div key={thread.id} role="row" className="set-tr" style={columns}>
                      <span role="cell">{thread.title}</span>
                      <span role="cell" className="t3">{new Date(thread.updated_at).toLocaleDateString("en-GB", { day: "numeric", month: "short" })}</span>
                      <span role="cell" className="row-actions">
                        {group.removed ? (
                          <span title="Add the project back to unarchive">
                            <Button small disabled>Unarchive</Button>
                          </span>
                        ) : (
                          <Button
                            small
                            onClick={() => {
                              void unarchiveThread(thread);
                              setThreads(threads.filter((entry) => entry.id !== thread.id));
                            }}
                          >
                            Unarchive
                          </Button>
                        )}
                        <Button small tone="danger" onClick={() => setDeleting(thread)}>
                          Delete
                        </Button>
                      </span>
                    </div>
                  ))}
                </div>
              </section>
          ))}
          {deleting ? (
            <ConfirmDialog title={`Delete “${deleting.title}”?`} confirmLabel="Delete thread" tone="danger" cancelFocus
              onClose={() => setDeleting(null)} onConfirm={() => remove(deleting)}>
              <p>The thread and its messages are removed from this computer. This can’t be undone.</p>
            </ConfirmDialog>
          ) : null}
        </>
      )}
    </>
  );
}
