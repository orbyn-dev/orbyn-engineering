import { useId, useState } from "react";
import { useAsync } from "../../lib/useAsync";
import { useUi } from "../../product/state/ui";
import { navigate } from "../../product/state/route";
import { useVisibleProjects, useWorkspace } from "../../product/state/workspace";
import { Button } from "../../ui/controls";
import { ConfirmDialog } from "../../ui/Dialog";
import { issuesApi, useApiVersion } from "./api";
import type { IssueDetail } from "./contract";
import { reloadIssueLinks } from "./links";
import { reasonOf, refKey } from "./model";

/**
 * Starts a thread from an issue. The first message is the service's framed draft (`GET /api/issues/draft`: the issue's text between
 * markers, as data, never as instructions), shown whole in a box the person can edit before anything is written. Reading the draft
 * writes nothing. One click on Start thread is one write, which creates the thread, links it and queues the message as it stands in
 * the box. The dialog's own busy state blocks a second click and Escape while the write runs; a failure keeps the dialog open and
 * says why. The title, number and the draft come from the tracker and are shown as text (the draft in a text box, never as markup).
 */
export function StartThreadDialog({ issue, onClose }: { issue: IssueDetail; onClose(): void }) {
  const projects = useVisibleProjects() ?? [];
  const version = useApiVersion();
  const draft = useAsync(`issues:draft:${version}:${refKey(issue)}`, (signal) => issuesApi().draft(issue, signal));
  // What the person typed, once they type; until then the box shows the draft as the service wrote it.
  const [edited, setEdited] = useState<string | null>(null);
  const [project, setProject] = useState(projects[0]?.id ?? "");
  const putThread = useWorkspace((state) => state.putThread);
  const pushToast = useUi((state) => state.pushToast);
  const messageId = useId();
  const projectId = useId();
  const message = edited ?? draft.data?.message ?? "";
  const ready = draft.status === "ready" && project !== "" && message.trim() !== "";
  return (
    <ConfirmDialog
      title={`Start a thread from ${issue.display_key}`}
      confirmLabel="Start thread"
      confirmDisabled={!ready}
      onClose={onClose}
      onConfirm={async () => {
        const thread = await issuesApi().startThread(issue, { message: message.trim(), project_id: project });
        putThread(thread);
        void reloadIssueLinks(version);
        pushToast(`Started a thread from ${issue.display_key}`);
        navigate({ name: "thread", id: thread.id });
      }}
    >
      <div className="iss-start">
        <div className="iss-start-issue"><span className="iss-quiet">Issue</span><span>{issue.display_key} {issue.title}</span></div>
        <label htmlFor={messageId}>First message</label>
        <textarea id={messageId} data-dialog-focus rows={8} value={message} readOnly={draft.status !== "ready"} aria-busy={draft.status === "loading" || undefined}
          placeholder={draft.status === "loading" ? "Reading the issue…" : undefined} onChange={(event) => setEdited(event.target.value)} />
        {draft.status === "error" ? (
          <div role="alert" className="iss-start-fail">
            <p className="field-error">Couldn’t read the issue. {reasonOf(draft.error)}</p>
            <Button small onClick={() => void draft.reload()}>Try again</Button>
          </div>
        ) : (
          <p className="iss-quiet">The issue’s text sits between the markers, as data for the engine to read. Change anything before you start.</p>
        )}
        <label htmlFor={projectId}>Project</label>
        <select id={projectId} value={project} onChange={(event) => setProject(event.target.value)}>
          {projects.length === 0 ? <option value="">No project</option> : null}
          {projects.map((one) => <option key={one.id} value={one.id}>{one.name}</option>)}
        </select>
        <p className="iss-quiet">The engine, model, permission and branch are the project’s defaults, as for a new thread.</p>
      </div>
    </ConfirmDialog>
  );
}
