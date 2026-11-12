import { TriangleAlert } from "lucide-react";
import { useState } from "react";
import type { RedactionProject } from "../../../generated/RedactionProject";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { ConfirmDialog, Dialog } from "../../../ui/Dialog";
import { Button } from "../../../ui/controls";
import { api } from "../api";

/**
 * Manage: every project with its state. Turn off… opens the warning (redact-off-project) with Cancel as the default;
 * Turn on is plain, with no confirm, because it only hides more.
 */
export function ProjectsOffDialog({
  off,
  onChange,
  onClose,
}: {
  off: RedactionProject[];
  onChange(id: string, redaction: boolean): Promise<unknown>;
  onClose(): void;
}) {
  const projects = useAsync("secrets-projects", (signal) => api.projects(signal));
  const [turningOff, setTurningOff] = useState<{ id: string; name: string } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const offIds = new Set(off.map((project) => project.id));

  const turnOn = async (id: string) => {
    setBusy(id);
    setError(null);
    try {
      await onChange(id, true);
    } catch (failure) {
      setError(toFailure(failure).message);
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <Dialog title="Projects with redaction off" onClose={onClose}>
        <p>Redaction is on in every project unless it is turned off here. Only the project you pick is affected.</p>
        {projects.error ? <p className="field-error" role="alert">{projects.error.message}</p> : null}
        {projects.data && !projects.data.length ? <p className="muted">There are no projects yet.</p> : null}
        <ul className="secrets-off-list" aria-label="Projects">
          {(projects.data ?? []).map((project) => {
            const isOff = offIds.has(project.id);
            return (
              <li key={project.id}>
                <span>
                  {project.name}
                  <small className="muted"> · {isOff ? "Redaction off" : "Redaction on"}</small>
                </span>
                {isOff ? (
                  <Button small busy={busy === project.id} aria-label={`Turn on redaction for ${project.name}`} onClick={() => void turnOn(project.id)}>Turn on</Button>
                ) : (
                  <Button small aria-label={`Turn off redaction for ${project.name}`} onClick={() => setTurningOff({ id: project.id, name: project.name })}>Turn off…</Button>
                )}
              </li>
            );
          })}
        </ul>
        {error ? <p className="field-error" role="alert">{error}</p> : null}
      </Dialog>
      {turningOff ? (
        <ConfirmDialog
          title={`Turn off redaction for ${turningOff.name}?`}
          confirmLabel="Turn off redaction"
          tone="danger"
          cancelFocus
          onConfirm={async () => { await onChange(turningOff.id, false); }}
          onClose={() => setTurningOff(null)}
        >
          <p>Only this project is affected. You can turn it back on at any time.</p>
          <p className="muted">Not applied yet: agents are not protected today in any project. This takes effect when hiding reaches what agents read.</p>
          <div className="secrets-warn" role="note">
            <TriangleAlert size={15} aria-hidden />
            <div>
              <b>Agents will see real secrets in this project.</b> Values such as keys and tokens in files, terminal output and
              environment variables are sent to the engine as they are.
            </div>
          </div>
        </ConfirmDialog>
      ) : null}
    </>
  );
}
