// Settings › Crew › Worker provider (Crew Designs v75, frames `settings-crew-worker-provider`, `-any` and `-narrow`): where Crew looks for
// a worker's engine for this project. Same as lead is the default. Nothing is drawn while there is no project to save it for, like the
// path classes beside it. The choice shown is the one the service stored: a write that fails leaves it as it was.
import { useRef, useState } from "react";
import { toFailure, useAsync } from "../../../../lib/useAsync";
import { Segmented } from "../../../../ui/Segmented";
import type { CrewWorkerProvider } from "../../../../generated/CrewWorkerProvider";
import { crewApi } from "./api";
import { useClassProject } from "./PathClasses";

const CHOICES: { value: CrewWorkerProvider; label: string }[] = [
  { value: "same_as_lead", label: "Same as lead" },
  { value: "any", label: "Any provider" },
];

function Row({ project }: { project: string }) {
  const read = useAsync(`crew-worker-provider:${project}`, (signal) => crewApi.workerProvider(project, signal));
  const [saved, setSaved] = useState<CrewWorkerProvider | null>(null);
  const [busy, setBusy] = useState(false);
  const inflight = useRef(false);
  const [failed, setFailed] = useState<string | null>(null);
  const shown = saved ?? read.data?.worker_provider ?? null;
  const choose = (next: CrewWorkerProvider) => {
    if (inflight.current || next === shown) return;
    inflight.current = true;
    setBusy(true);
    setFailed(null);
    // The row is keyed by project, so a project change unmounts it and a late answer lands nowhere.
    crewApi.setWorkerProvider(project, next).then(
      (view) => setSaved(view.worker_provider),
      (error) => setFailed(toFailure(error).message),
    ).finally(() => { inflight.current = false; setBusy(false); });
  };
  return (
    <section className="wp-row" aria-label="Worker provider">
      <div>
        <b>Worker provider</b>
        <small>Same as lead keeps workers and scouts on the lead’s provider. Any provider lets Crew pick among the engines that can run workers. Review lanes always use another provider; this row does not change that.</small>
      </div>
      {shown ? (
        <div className="wp-seg" aria-busy={busy}>
          <Segmented label="Worker provider" value={shown} options={CHOICES} onChange={choose} />
        </div>
      ) : null}
      {read.status === "error" && !shown ? <p role="alert" className="wp-error">{read.error.message}</p> : null}
      {failed ? <p role="alert" className="wp-error">{failed}</p> : null}
    </section>
  );
}

/** Settings › Crew › Worker provider, for the project of the checkout Settings shows. Nothing is drawn with no project. */
export function WorkerProvider() {
  const project = useClassProject();
  return project ? <Row project={project.id} key={project.id} /> : null;
}
