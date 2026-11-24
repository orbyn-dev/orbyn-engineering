import { useState } from "react";
import { ConfirmDialog } from "../../../ui/Dialog";
import { plain } from "../flow/live/model";
import { leadApi } from "./api";
import { useCloseLead, type CloseLeadRequest } from "./closeLead";
import "./closeLead.css";

const tasksWord = (count: number) => (count === 1 ? "task" : "tasks");

function CloseLeadConfirm({ request }: { request: CloseLeadRequest }) {
  const { thread, verb, tasks, done } = request;
  const [busy, setBusy] = useState(false);
  const count = tasks.length;
  const verbing = verb === "settle" ? "Settling" : "Archiving";

  const close = async () => {
    setBusy(true);
    try {
      const answer = await leadApi.close(thread.id);
      done({ kind: "closed", ...answer, tasks });
    } catch (failure) {
      // ConfirmDialog shows its own error, and the dialog stays open for another try or a cancel.
      setBusy(false);
      throw failure;
    }
  };

  return (
    <ConfirmDialog
      title={`${verb === "settle" ? "Settle" : "Archive"} “${thread.title}”?`}
      confirmLabel={`${verb === "settle" ? "Settle" : "Archive"} and cancel ${count} ${tasksWord(count)}`}
      tone="danger"
      keepOpen
      cancelFocus
      className="close-lead"
      onConfirm={close}
      onClose={() => {
        if (!busy) done({ kind: "declined" });
      }}
    >
      <p>
        {`It started ${count} ${tasksWord(count)} that ${count === 1 ? "is" : "are"} still live. ${verbing} the thread cancels each one: its worker is stopped and the task ends as cancelled. Work already merged stays.`}
      </p>
      <h3 className="close-lead-heading">{`Will be cancelled (${count})`}</h3>
      <ul className="close-lead-tasks">
        {tasks.slice(0, 5).map((task) => (
          <li className="close-lead-task" key={task.id}>
            <span className="close-lead-name">{task.name}</span>
            <span className="close-lead-state">{plain(task.state)}</span>
          </li>
        ))}
      </ul>
      {count > 5 ? <p className="close-lead-more">{`and ${count - 5} more`}</p> : null}
      <p className="close-lead-muted">Files the workers changed stay as they are.</p>
      <p className="close-lead-muted">A cancelled task is final. It is never restarted; new work needs a new brief.</p>
      {busy ? (
        <p className="close-lead-busy">
          {`Cancelling ${count} ${tasksWord(count)}, one by one. The answer comes when every task has been tried.`}
        </p>
      ) : null}
    </ConfirmDialog>
  );
}

/** The one close confirm the shell mounts. Nothing is drawn unless `askCloseLead` has a request waiting. */
export function CloseLeadDialog() {
  const pending = useCloseLead((state) => state.pending);
  if (!pending) return null;
  return <CloseLeadConfirm key={`${pending.verb}:${pending.thread.id}`} request={pending} />;
}
