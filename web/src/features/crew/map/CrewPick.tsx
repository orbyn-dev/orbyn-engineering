import type { ReactNode } from "react";
import { ErrorState, Empty, Loading } from "../../../product/ui/states";
import { useFlowCrews } from "../flow/live/useFlow";
import { useState } from "react";

/** The crew the map or the lanes read: a select of the crews that have work, then `children` for the chosen one. */
export function CrewPick({ label, id, children }: { label: string; id: string; children(crew: string): ReactNode }) {
  const crews = useFlowCrews();
  const [picked, setPicked] = useState<string | null>(null);
  if (crews.status === "loading") return <Loading label={`Reading the ${label}`} rows={4} />;
  if (crews.status === "error" && !crews.data) return <ErrorState title={`Can’t read the ${label}`} failure={crews.error} onRetry={() => void crews.reload()} />;
  const list = crews.data ?? [];
  if (list.length === 0) return <Empty title="No crews running" />;
  const crew = list.find((one) => one.id === picked) ?? list[0]!;
  return (
    <>
      <div className="mp-crew">
        <label htmlFor={id}>Crew</label>
        <select id={id} value={crew.id} onChange={(event) => setPicked(event.target.value)}>
          {list.map((one) => (
            <option key={one.id} value={one.id}>
              {one.label} · {one.tasks} {one.tasks === 1 ? "task" : "tasks"}, {one.finished} finished
            </option>
          ))}
        </select>
      </div>
      {children(crew.id)}
    </>
  );
}
