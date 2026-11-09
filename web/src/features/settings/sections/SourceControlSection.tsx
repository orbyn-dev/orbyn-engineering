import { Check, CloudOff, RefreshCw } from "lucide-react";
import { settingsLabel } from "../settingsSearch";
import { useRef } from "react";
import { useConnection } from "../../../product/state/connection";
import { request } from "../api";
import type { ForgeStatus } from "../../../api/types";
import { useAsync } from "../../../lib/useAsync";
import { Button } from "../../../ui/controls";
import { Empty, ErrorState, Loading } from "../../../product/ui/states";
import { PreferenceGroups } from "../rows/PreferenceRows";
import { SourceControlWriting } from "./SourceControlWriting";
import { heading } from "./parts";

export function SourceControlSection() {
  const forges = useAsync("forges", (signal) => request<ForgeStatus[]>("GET", "/api/forges?rescan=true", undefined, signal));
  const offline = useConnection((state) => state.offline);
  const scannedAt = useRef<number | null>(null);
  if (forges.status === "ready" && !offline) scannedAt.current = Date.now();
  return (
    <>
      {heading("Source control")}
      <SourceControlWriting />
      <PreferenceGroups groups={[{ title: "", rows: [{
        key: "auto_pull", title: settingsLabel("source-control-automatic-pull"),
        description: "Keep tracked branches up to date in the background",
      }] }]} />
      <div className="set-hosts-head">
        <b>Hosts</b>
        <Button small disabled={offline} icon={<RefreshCw size={13} aria-hidden />} onClick={forges.reload}>Rescan</Button>
      </div>
      {offline ? <p className="banner" role="status"><CloudOff size={14} aria-hidden />You’re offline. Showing the last scan{scannedAt.current ? ` from ${new Date(scannedAt.current).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false })}` : ""}. Rescan and sign-in are unavailable.</p> : null}
      {forges.status === "loading" ? (
        <><span className="sr-only" role="status">Rescanning your remotes…</span><Loading label="Checking source control" rows={3} /></>
      ) : forges.status === "error" ? (
        <ErrorState title="Couldn't check source control" failure={forges.error} onRetry={forges.reload} />
      ) : forges.data.length === 0 ? (
        <Empty title="No Git hosts found"><p>Add a project with a remote and they appear here.</p>
          <Button small disabled={offline} icon={<RefreshCw size={13} aria-hidden />} onClick={forges.reload}>Rescan</Button></Empty>
      ) : (
        <div className="set-tbl" role="table" aria-label="Hosts">
          {forges.data.map((forge) => (
            <div key={forge.id} role="row" className="set-tr" style={{ gridTemplateColumns: "28px minmax(0, 1fr) 150px" }}>
              <span role="cell"><span className="set-mark">{forge.name.charAt(0).toUpperCase()}</span></span>
              <span role="cell"><span className="name">{forge.name}</span>{forge.problem ?? forge.accounts.join(", ") ? <><br /><span className="sub">{forge.problem ?? forge.accounts.join(", ")}</span></> : null}</span>
              <span role="cell">
                {forge.problem && !forge.signed_in ? <span className="set-pill warn">Can’t reach {forge.name}</span>
                  : forge.signed_in ? <span className="set-pill"><Check size={13} aria-hidden />Signed in</span>
                  : <span className="set-pill">{forge.installed ? "Not signed in" : "Not installed"}</span>}
              </span>
            </div>
          ))}
        </div>
      )}
    </>
  );
}
