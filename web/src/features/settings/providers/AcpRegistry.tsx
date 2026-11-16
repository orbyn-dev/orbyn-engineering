import { AlertTriangle, CloudOff, RefreshCw, Search } from "lucide-react";
import { useRef, useState } from "react";
import { ApiFailure } from "../../../api/client";
import { addRegistryEntry, listRegistry, removeRegistryEntry } from "../api";
import type { AcpRegistryEntry } from "../../../api/types";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useConnection } from "../../../product/state/connection";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { Button } from "../../../ui/controls";
import { ConfirmDialog } from "../../../ui/Dialog";
import { Empty, Loading } from "../../../product/ui/states";
import { useEngineUpdates } from "./engineUpdates";
import { BackBar } from "./ProviderViews";

const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });

/** How an entry installs, as the row says it: what runs, and when anything is downloaded. */
const HOW: Record<string, { pill: string; line: string }> = {
  binary: { pill: "Binary", line: "Checksummed binary" },
  npx: { pill: "npx", line: "Runs with npx · downloads when you start it" },
  uvx: { pill: "uvx", line: "Runs with uvx · downloads when you start it" },
};

/** The service names the reason in its own words; a refused removal is said in the dialog's. */
function removalFailure(name: string, failure: unknown) {
  const reason = toFailure(failure);
  return reason.code === "engine_busy" ? new ApiFailure(reason.code, `A thread is running on ${name}. Stop it, then remove it.`, reason.status) : reason;
}

/** Agents that speak ACP: what each installs with, its state, and Install or Remove. */
export function AcpRegistryView({ onBack }: { onBack(): void }) {
  const [text, setText] = useState("");
  const [removing, setRemoving] = useState<AcpRegistryEntry | null>(null);
  const offline = useConnection((state) => state.offline);
  const engines = useWorkspace((state) => state.engines.data);
  const refreshEngines = useWorkspace((state) => state.refreshEngines);
  const pushToast = useUi((state) => state.pushToast);
  const running = useEngineUpdates((state) => state.running);
  const [installing, setInstalling] = useState<string | null>(null);
  const loadedAt = useRef<number | null>(null);
  const found = useAsync("acp-registry", async (signal) => {
    const entries = await listRegistry("", signal);
    loadedAt.current = Date.now();
    return entries;
  }, { keep: true });
  const needle = text.trim().toLowerCase();
  const rows = (found.data ?? []).filter((entry) => `${entry.name} ${entry.description}`.toLowerCase().includes(needle));
  const install = async (entry: AcpRegistryEntry) => {
    setInstalling(entry.id);
    try {
      await addRegistryEntry(entry.id, entry.installable[0]);
      await refreshEngines();
      found.reload();
      pushToast(`${entry.name} installed`);
    } catch (failure) {
      pushToast(`Couldn't install ${entry.name}. ${toFailure(failure).message}`);
    } finally { setInstalling(null); }
  };
  return (
    <>
      <BackBar title="ACP registry" onBack={onBack}>
        <Button small icon={<RefreshCw size={13} aria-hidden />} busy={found.refreshing && found.status === "ready"} onClick={found.reload}>Refresh</Button>
      </BackBar>
      <label className="set-find">
        <Search size={13} aria-hidden />
        <input aria-label="Search the registry" placeholder="Search the registry" value={text} onChange={(event) => setText(event.target.value)} />
      </label>
      {offline ? (
        <p className="banner" role="status"><CloudOff size={14} aria-hidden />You're offline.{loadedAt.current !== null ? ` Showing the list from ${clock(loadedAt.current)}.` : ""}</p>
      ) : null}
      {found.status === "loading" ? <Loading label="Loading the registry" rows={4} /> : found.status === "error" && !found.data ? (
        offline ? null : (
          <p className="banner banner-err" role="alert"><AlertTriangle size={14} aria-hidden />Couldn't load the registry. The server answered with an error.
            <Button small className="set-back-end" icon={<RefreshCw size={13} aria-hidden />} onClick={found.reload}>Retry</Button></p>
        )
      ) : rows.length === 0 ? (
        needle ? <Empty title={`No agent matches “${text.trim()}”`}><p>Check the spelling or clear the search.</p></Empty> : <Empty title="The registry is empty" />
      ) : (
        <div className="set-tbl" role="table" aria-label="ACP registry">
          {rows.map((entry) => {
            const how = HOW[entry.installable[0] ?? ""];
            const version = entry.engine ? engines?.find((engine) => engine.id === entry.engine)?.version : null;
            const busy = installing === entry.id || (entry.engine !== null && entry.engine in running);
            return (
              <div key={entry.id} role="row" className="set-tr" style={{ gridTemplateColumns: "28px minmax(0, 1fr) auto auto" }}>
                <span role="cell"><span className="set-mark">{entry.name.charAt(0).toUpperCase()}</span></span>
                <span role="cell">
                  <span className="name">{entry.name}</span><br />
                  <span className="sub">{entry.engine ? `Installed${version ? ` · ${version}` : ""}` : how ? how.line : entry.description}</span>
                </span>
                <span role="cell"><span className="set-pill">{entry.engine ? "Installed" : how ? how.pill : "Not available"}</span></span>
                <span role="cell" className="row-actions">
                  {entry.engine ? (
                    <Button small tone="quiet" aria-label={`Remove ${entry.name}`} onClick={() => setRemoving(entry)}>Remove…</Button>
                  ) : entry.installable.length > 0 ? (
                    <Button small aria-label={`Install ${entry.name}`} busy={busy} disabled={busy} onClick={() => void install(entry)}>{busy ? "Installing…" : "Install"}</Button>
                  ) : <span className="t3">Can't be installed here</span>}
                </span>
              </div>
            );
          })}
        </div>
      )}
      {removing ? (
        <ConfirmDialog title={`Remove ${removing.name}?`} confirmLabel="Remove" tone="danger" onClose={() => setRemoving(null)}
          onConfirm={async () => {
            try { await removeRegistryEntry(removing.id); } catch (failure) { throw removalFailure(removing.name, failure); }
            await refreshEngines();
            found.reload();
            pushToast(`${removing.name} removed`);
          }}>
          <p>It leaves Orbyn’s list. Its sign-in and sessions stay with the engine.</p>
        </ConfirmDialog>
      ) : null}
    </>
  );
}
