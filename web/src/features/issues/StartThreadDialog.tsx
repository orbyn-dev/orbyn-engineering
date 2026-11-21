import { useId, useState } from "react";
import { useUi } from "../../product/state/ui";
import { navigate } from "../../product/state/route";
import { useVisibleProjects, useWorkspace } from "../../product/state/workspace";
import { ConfirmDialog } from "../../ui/Dialog";
import { issuesApi, useApiVersion } from "./api";
import type { IssueDetail } from "./contract";
import { reloadIssueLinks } from "./links";

/** The first message the dialog proposes: the number and title, for the person to change before they start. */
export const firstMessage = (issue: Pick<IssueDetail, "display_key" | "title">) =>
  `Work on issue ${issue.display_key}: ${issue.title}. Read the issue, find the cause, and fix it.`;

/**
 * Starts a thread from an issue: one click on Start thread is one write, which creates the thread and links it. The
 * dialog's own busy state blocks a second click and Escape while the write runs; a failure keeps the dialog open and
 * says why. The title and number come from the tracker and are shown as text.
 */
export function StartThreadDialog({ issue, onClose }: { issue: IssueDetail; onClose(): void }) {
  const projects = useVisibleProjects() ?? [];
  const [message, setMessage] = useState(() => firstMessage(issue));
  const [project, setProject] = useState(projects[0]?.id ?? "");
  const version = useApiVersion();
  const putThread = useWorkspace((state) => state.putThread);
  const pushToast = useUi((state) => state.pushToast);
  const messageId = useId();
  const projectId = useId();
  const ready = project !== "" && message.trim() !== "";
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
        <textarea id={messageId} data-dialog-focus rows={4} value={message} onChange={(event) => setMessage(event.target.value)} />
        <p className="iss-quiet">Filled from the issue. Change it before you start.</p>
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
