import { Check, ChevronLeft, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import type { EngineStatus } from "../../../api/types";
import type { EngineUpdateOutcome } from "../../../generated/EngineUpdateOutcome";
import { toFailure } from "../../../lib/useAsync";
import { useWorkspace } from "../../../product/state/workspace";
import { Button } from "../../../ui/controls";
import { SettingsBanner } from "../rows/SettingsBanner";
import { useEngineUpdates } from "./engineUpdates";

/** What an update of every engine with a newer version leaves in each engine's row. */
type Result = { version: string | null; error: string | null };

/** The engines the confirmation lists as skipped: newer, but no installer Orbyn recognises. */
export const skippedNoInstaller = (engines: EngineStatus[]) =>
  engines.filter((engine) => !engine.instance_of && !engine.update_command && !engine.compatibility && engine.latest !== null && engine.latest !== engine.version);

/** The dialog's body: each engine with its versions and the command it runs, then what is skipped. */
export function UpdateAllFacts({ engines, skipped }: { engines: EngineStatus[]; skipped: EngineStatus[] }) {
  return (
    <>
      <div className="set-update-cmds">
        {engines.map((engine) => (
          <div key={engine.id}>
            <b>{engine.name}</b> {engine.version} → {engine.latest}
            <code>{engine.update_command}</code>
          </div>
        ))}
      </div>
      {skipped.length > 0 ? <p className="t3">Skipped: {skipped.map((engine) => `${engine.name} has no installer Orbyn recognises.`).join(" ")}</p> : null}
    </>
  );
}

function Row({ engine, result, line, running, final, retry }: { engine: EngineStatus; result: Result | undefined; line: string | undefined; running: boolean; final: boolean; retry: (() => void) | null }) {
  const failed = result?.error != null;
  const done = result !== undefined && !failed;
  return (
    <div className="set-tr set-update-row" role="listitem">
      <span className="set-mark">{engine.name.charAt(0).toUpperCase()}</span>
      <span>
        <span className="name">{engine.name}</span>
        <br />
        {failed ? <span className="sub set-update-fail">{result.error}<span className="t3"> · still on {engine.version}</span></span>
          : running || (done && !final && line) ? <span className="sub mono">{line}</span>
          : <span className="sub">{done ? result.version ?? engine.latest ?? engine.version : engine.version}</span>}
      </span>
      <span className="set-update-end">
        {failed ? <><span className="set-pill bad">Failed</span>{retry ? <Button small onClick={retry}>Try again</Button> : null}</>
          : running ? <span className="set-pill"><Loader2 size={13} className="spin" aria-hidden />Updating…</span>
          : done ? <span className="set-pill good"><Check size={13} aria-hidden />{!final && result.version ? `Updated to ${result.version}` : "Updated"}</span>
          : null}
      </span>
    </div>
  );
}

/**
 * Engines update one after another: each row shows its command and latest line, then its result as it ends
 * (updateall-running). When every one has ended a failure is named with its reason, and Try again runs only that
 * engine (updateall-partial). An update can't be stopped, so closing this page leaves it running.
 */
export function UpdateAllView({ engines, onBack }: { engines: EngineStatus[]; onBack(): void }) {
  const refreshEngines = useWorkspace((state) => state.refreshEngines);
  const live = useEngineUpdates((state) => state.running);
  const said = useEngineUpdates((state) => state.said);
  const ended = useEngineUpdates((state) => state.ended);
  const [outcomes, setOutcomes] = useState<Record<string, Result> | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const started = useRef(false);
  const [retrying, setRetrying] = useState<string[]>([]);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    useEngineUpdates.setState({ said: {}, ended: {} });
    api.updateAllEngines().then(
      (list: EngineUpdateOutcome[]) => { setOutcomes(Object.fromEntries(list.map((entry) => [entry.id, { version: entry.version ?? null, error: entry.error ?? null }]))); void refreshEngines(); },
      (reason: unknown) => { setFailure(toFailure(reason).message); void refreshEngines(); },
    );
  }, [refreshEngines]);
  const result = (id: string): Result | undefined => outcomes?.[id] ?? ended[id];
  const failedCount = engines.filter((engine) => result(engine.id)?.error != null).length;
  const finished = outcomes !== null && retrying.length === 0;
  const retry = (engine: EngineStatus) => {
    setRetrying((now) => [...now, engine.id]);
    api.updateEngine(engine.id).then(
      () => setOutcomes((now) => ({ ...now, [engine.id]: { version: null, error: null } })),
      (reason: unknown) => setOutcomes((now) => ({ ...now, [engine.id]: { version: null, error: toFailure(reason).message } })),
    ).finally(() => { setRetrying((now) => now.filter((id) => id !== engine.id)); void refreshEngines(); });
  };
  return (
    <>
      <div className="set-back">
        <Button small tone="quiet" icon={<ChevronLeft size={13} aria-hidden />} onClick={onBack}>Providers</Button>
        <h2 className="set-back-title">Updating engines</h2>
      </div>
      {failure ? <SettingsBanner tone="err">{failure}</SettingsBanner>
        : finished && failedCount > 0 ? <SettingsBanner tone="err">{failedCount} of {engines.length} engines couldn’t be updated.</SettingsBanner> : null}
      <div className="set-tbl" role="list" aria-label="Engines">
        {engines.map((engine) => (
          <Row key={engine.id} engine={engine} result={result(engine.id)} line={said[engine.id]}
            running={engine.id in live || retrying.includes(engine.id)} final={finished}
            retry={failure === null && retrying.length === 0 ? () => retry(engine) : null} />
        ))}
      </div>
      {finished || failure ? null : <p className="t3 set-foot">An update can’t be stopped once it starts: stopping an installer halfway could leave the engine half-written. You can close this page; it keeps going.</p>}
    </>
  );
}
