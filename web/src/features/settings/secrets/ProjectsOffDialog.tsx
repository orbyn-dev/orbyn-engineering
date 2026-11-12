import { TriangleAlert } from "lucide-react";
import { useState } from "react";
import type { RedactionProject } from "../../../generated/RedactionProject";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { Dialog } from "../../../ui/Dialog";
import { Button, Field } from "../../../ui/controls";
import { api } from "../api";

/**
 * Turn redaction off for one project (redact-off-project): the warning says what changes, Cancel has the focus, and the
 * projects that are off are listed with a way back on.
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
  const [chosen, setChosen] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const offIds = new Set(off.map((project) => project.id));
  const choices = (projects.data ?? []).filter((project) => !offIds.has(project.id));
  const target = choices.find((project) => project.id === chosen) ?? choices[0];

  const run = async (id: string, redaction: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await onChange(id, redaction);
      if (!redaction) onClose();
    } catch (failure) {
      setError(toFailure(failure).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={target ? `Turn off redaction for ${target.name}?` : "Projects with redaction off"}
      onClose={onClose}
      busy={busy}
      footer={
        <>
          <Button onClick={onClose} disabled={busy} data-dialog-focus>Cancel</Button>
          <Button tone="danger" busy={busy} disabled={!target} onClick={() => target && void run(target.id, false)}>
            Turn off redaction
          </Button>
        </>
      }
    >
      <p>Only this project is affected. You can turn it back on at any time.</p>
      <div className="secrets-warn" role="note">
        <TriangleAlert size={15} aria-hidden />
        <div>
          <b>Agents will see real secrets in this project.</b> Values such as keys and tokens in files, terminal output and
          environment variables are sent to the engine as they are.
        </div>
      </div>
      {projects.error ? <p className="field-error" role="alert">{projects.error.message}</p> : null}
      <Field label="Project">
        {(props) => (
          <select {...props} value={target?.id ?? ""} onChange={(event) => setChosen(event.target.value)} disabled={!choices.length}>
            {choices.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}
          </select>
        )}
      </Field>
      {off.length ? (
        <ul className="secrets-off-list" aria-label="Projects with redaction off">
          {off.map((project) => (
            <li key={project.id}>
              <span>{project.name}</span>
              <Button small disabled={busy} aria-label={`Turn redaction back on for ${project.name}`} onClick={() => void run(project.id, true)}>Turn on</Button>
            </li>
          ))}
        </ul>
      ) : null}
      {error ? <p className="field-error" role="alert">{error}</p> : null}
    </Dialog>
  );
}
