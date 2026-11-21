import { useRef, useState } from "react";
import { Button } from "../../../ui/controls";
import { ConfirmDialog } from "../../../ui/Dialog";
import { useNow } from "../../../lib/useNow";
import { SettingRow } from "../rows/SettingRow";
import { type HostRow, engineHostApi, useEngineHosts, isProcess, plain, processLine, STOPPED_SENTENCE } from "../../enginehost";
import { reasonOf } from "./HostParts";

/** The hosts that are a process now. Empty while the service reports none, which leaves Processes as it is today. */
export function useHostProcesses(): { rows: HostRow[]; reload(): void } {
  const { hosts, reload } = useEngineHosts();
  return { rows: (hosts ?? []).filter(isProcess), reload };
}

function Row({ row, reload }: { row: HostRow; reload(): void }) {
  const now = useNow(row.state === "running");
  const [stopping, setStopping] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // One click, one write: a second click while the first is in flight is not another request.
  const pending = useRef(false);
  const name = `${plain(row.name, 60)} engine host`;
  const restart = async () => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try { await engineHostApi().retry(row.id); reload(); }
    catch (failure) { setError(reasonOf(failure)); }
    finally { pending.current = false; setBusy(false); }
  };
  const stop = async () => {
    if (pending.current) return;
    pending.current = true;
    try { await engineHostApi().stop(row.id); reload(); }
    finally { pending.current = false; }
  };
  const canStop = row.state !== "cant_start";
  const canRestart = row.state === "running" || row.state === "cant_start";
  const threads = row.running_threads ?? 0;
  return (
    <li>
      <SettingRow title={name} description={processLine(row, now)}>
        {canRestart ? <Button small tone="quiet" busy={busy} aria-label={`Restart ${name}`} onClick={() => void restart()}>Restart</Button> : null}
        {canStop ? <button type="button" className="set-red-link" aria-label={`Stop ${name}`} onClick={() => setStopping(true)}>Stop</button> : null}
      </SettingRow>
      {error ? <p className="field-error eh-row-error" role="alert">{error}</p> : null}
      {stopping ? (
        <ConfirmDialog title={`Stop the ${plain(row.name, 60)} engine host?`} confirmLabel="Stop" tone="danger" cancelFocus onClose={() => setStopping(false)} onConfirm={stop}>
          <p>It stops now. {threads > 0 ? `The ${threads === 1 ? "thread" : `${threads} threads`} running on it end${threads === 1 ? "s" : ""} with “${STOPPED_SENTENCE}” ` : "Threads running on it end with the host’s own sentence. "}It starts again with the next message.</p>
        </ConfirmDialog>
      ) : null}
    </li>
  );
}

/** The engine hosts as rows of Maintenance › Processes, above the ordinary processes, with the same red Stop. */
export function HostProcessRows({ rows, reload }: { rows: HostRow[]; reload(): void }) {
  return <>{rows.map((row) => <Row key={row.id} row={row} reload={reload} />)}</>;
}
