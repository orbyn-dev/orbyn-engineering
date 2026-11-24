import { useRef, useState } from "react";
import { ApiFailure } from "../../../api/client";
import { Button } from "../../../ui/controls";
import { ConfirmDialog } from "../../../ui/Dialog";
import { useNow } from "../../../lib/useNow";
import { SettingRow } from "../rows/SettingRow";
import { type HostRow, engineHostApi, hostWritable, isBackgroundOpen, useEngineHosts, isProcess, plain, processLine, STOPPED_SENTENCE } from "../../enginehost";
import { reasonOf } from "./HostParts";

/** The hosts that are a process now. Empty while the service reports none, which leaves Processes as it is today. */
export function useHostProcesses(): { rows: HostRow[]; reload(): void } {
  const { hosts, reload } = useEngineHosts();
  return { rows: (hosts ?? []).filter(isProcess), reload };
}

function Row({ row, reload }: { row: HostRow; reload(): void }) {
  const now = useNow(row.state === "running");
  const [stopping, setStopping] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The question after a 409 `background_open`: the service's own sentence, and the one action that may be sent again.
  const [ask, setAsk] = useState<{ action: "stop" | "restart"; message: string } | null>(null);
  // One click, one write: a second click while the first is in flight is not another request.
  const pending = useRef(false);
  const name = `${plain(row.name, 60)} engine host`;
  const restart = async (stopBackground = false) => {
    if (pending.current) return;
    pending.current = true;
    setRestarting(false);
    setAsk(null);
    setBusy(true);
    setError(null);
    try { await engineHostApi().retry(row.id, stopBackground ? { stopBackground: true } : undefined); reload(); }
    catch (failure) {
      // Background work is open: ask once; only the person's confirm sends it again, with `stop_background`.
      if (!stopBackground && isBackgroundOpen(failure)) setAsk({ action: "restart", message: reasonOf(failure) });
      else setError(reasonOf(failure));
    }
    finally { pending.current = false; setBusy(false); }
  };
  const stop = async (stopBackground = false) => {
    if (pending.current) return;
    pending.current = true;
    try { await engineHostApi().stop(row.id, stopBackground ? { stopBackground: true } : undefined); reload(); setAsk(null); }
    catch (failure) {
      // The first question closes and the background question takes its place; it is never answered for the person.
      if (!stopBackground && isBackgroundOpen(failure)) { setAsk({ action: "stop", message: reasonOf(failure) }); return; }
      // The dialog shows the failure's message as it is; text the service chose is cut first.
      throw new ApiFailure("host_write_failed", reasonOf(failure), 0);
    }
    finally { pending.current = false; }
  };

  // Stop and restart are Person routes of a later slice: with none, the row says what runs and has no button.
  const writable = hostWritable();
  const canStop = writable && row.state !== "cant_start";
  const canRestart = writable && (row.state === "running" || row.state === "cant_start");
  const threads = row.running_threads ?? 0;
  // Restarting a running host ends its threads the way Stop does. The frames draw no question for Restart; one is asked anyway
  // when threads would end, and a host with none restarts at once.
  const askRestart = row.state === "running" && threads > 0;
  return (
    <li>
      <SettingRow title={name} description={processLine(row, now)}>
        {canRestart ? <Button small tone="quiet" busy={busy} aria-label={`Restart ${name}`} onClick={() => (askRestart ? setRestarting(true) : void restart())}>Restart</Button> : null}
        {canStop ? <button type="button" className="set-red-link" aria-label={`Stop ${name}`} onClick={() => setStopping(true)}>Stop</button> : null}
      </SettingRow>
      {error ? <p className="field-error eh-row-error" role="alert">{error}</p> : null}
      {restarting ? (
        <ConfirmDialog title={`Restart the ${plain(row.name, 60)} engine host?`} confirmLabel="Restart" tone="danger" cancelFocus onClose={() => setRestarting(false)} onConfirm={() => restart()}>
          <p>It stops now and starts again. The {threads === 1 ? "thread" : `${threads} threads`} running on it end{threads === 1 ? "s" : ""} with “{STOPPED_SENTENCE}”</p>
        </ConfirmDialog>
      ) : null}
      {stopping ? (
        <ConfirmDialog title={`Stop the ${plain(row.name, 60)} engine host?`} confirmLabel="Stop" tone="danger" cancelFocus onClose={() => setStopping(false)} onConfirm={() => stop()}>
          <p>It stops now. {threads > 0 ? `The ${threads === 1 ? "thread" : `${threads} threads`} running on it end${threads === 1 ? "s" : ""} with “${STOPPED_SENTENCE}” ` : "Threads running on it end with the host’s own sentence. "}It starts again with the next message.</p>
        </ConfirmDialog>
      ) : null}
      {ask ? (
        <ConfirmDialog title={`${ask.action === "stop" ? "Stop" : "Restart"} the ${plain(row.name, 60)} engine host anyway?`} confirmLabel={ask.action === "stop" ? "Stop anyway" : "Restart anyway"} tone="danger" cancelFocus onClose={() => setAsk(null)}
          onConfirm={() => (ask.action === "stop" ? stop(true) : restart(true))}>
          <p>{plain(ask.message, 200)}</p>
        </ConfirmDialog>
      ) : null}
    </li>
  );
}

/** The engine hosts as rows of Maintenance › Processes, above the ordinary processes, with the same red Stop. */
export function HostProcessRows({ rows, reload }: { rows: HostRow[]; reload(): void }) {
  return <>{rows.map((row) => <Row key={row.id} row={row} reload={reload} />)}</>;
}
