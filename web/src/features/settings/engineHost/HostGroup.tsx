import { useRef, useState } from "react";
import { Button } from "../../../ui/controls";
import type { HostRow } from "./contract";
import { engineHostApi, useEngineHosts } from "./hostApi";
import { listLine } from "./hostText";
import { HostChip, reasonOf } from "./HostParts";
import "./engineHost.css";

/** The first letter of an engine's name, as the provider rows' mark. */
const mark = (name: string) => name.trim().charAt(0).toUpperCase();

/** Try again: the one action on this group, one request per click. It stays on the row if the request fails. */
function TryAgain({ row, onDone }: { row: HostRow; onDone(): void }) {
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const click = async () => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try { await engineHostApi().retry(row.id); onDone(); }
    catch (failure) { setError(reasonOf(failure)); }
    finally { pending.current = false; setBusy(false); }
  };
  return (
    <>
      <Button small busy={busy} onClick={() => void click()}>Try again</Button>
      {error ? <p className="field-error eh-row-error" role="alert">{error}</p> : null}
    </>
  );
}

/**
 * The Engine host group at the end of the Providers page: one row per engine family, with its state chip and one
 * line about what it is doing. Nothing is drawn until the service reports at least one host.
 */
export function EngineHostGroup() {
  const { hosts, reload } = useEngineHosts();
  if (!hosts || hosts.length === 0) return null;
  return (
    <>
      <div className="set-grp">Engine host</div>
      <p className="setting-line-desc eh-caption">One helper program runs each engine family. Orbyn starts it, stops it when it is idle and restarts it if it crashes.</p>
      <div className="set-tbl eh-table" role="table" aria-label="Engine host">
        {hosts.map((row) => (
          <div key={row.id} role="row" className="set-tr">
            <span role="cell"><span className="set-mark">{mark(row.name)}</span></span>
            <span role="cell"><span className="name">{row.name}</span><br /><span className="sub eh-sub">{listLine(row)}</span></span>
            <span role="cell" className="eh-chips"><HostChip state={row.state} /></span>
            <span role="cell">{row.can_retry ? <TryAgain row={row} onDone={reload} /> : null}</span>
          </div>
        ))}
      </div>
    </>
  );
}
