import { ChevronLeft } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, importHistorySession, scanHistory } from "../api";
import type { HistoryScan } from "../../../generated/HistoryScan";
import type { HistorySession } from "../../../generated/HistorySession";
import { toFailure } from "../../../lib/useAsync";
import { navigate } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { Button } from "../../../ui/controls";
import { Empty, Loading } from "../../../product/ui/states";

const day = (iso: string) => new Date(iso).toLocaleDateString([], { day: "numeric", month: "short" });
const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;

export function BackBar({ title, onBack, children }: { title: string; onBack(): void; children?: React.ReactNode }) {
  return (
    <div className="set-back">
      <Button small tone="quiet" icon={<ChevronLeft size={13} aria-hidden />} onClick={onBack}>Providers</Button>
      <h2 className="set-back-title">{title}</h2>
      {children ? <span className="set-back-end">{children}</span> : null}
    </div>
  );
}

/** Claude and Codex sessions kept outside Orbyn, picked and imported as threads. */
export function HistoryImportView({ onBack }: { onBack(): void }) {
  const pushToast = useUi((state) => state.pushToast);
  const [scan, setScan] = useState<HistoryScan | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);
  const keyOf = (session: HistorySession) => `${session.engine}:${session.session_id}`;
  const find = async () => {
    setBusy("scan");
    try {
      const next = await scanHistory();
      setScan(next);
      setPicked(new Set(next.sessions.filter((session) => session.importable && !session.thread_id).map(keyOf)));
    } catch (failure) {
      pushToast(`Couldn't read agent history: ${toFailure(failure).message}`);
    } finally { setBusy(null); }
  };
  const scanned = useRef(false);
  useEffect(() => {
    if (scanned.current) return;
    scanned.current = true;
    void find();
  });
  const available = (scan?.sessions ?? []).filter((session) => session.importable && !session.thread_id);
  const importPicked = async () => {
    setBusy("import");
    let last: string | null = null, done = 0;
    for (const session of available.filter((entry) => picked.has(keyOf(entry)))) {
      try {
        const result = await importHistorySession(session.engine, session.session_id);
        last = result.thread_id; done += 1;
      } catch (failure) { pushToast(`Couldn't import ${session.title}: ${toFailure(failure).message}`); }
    }
    await useWorkspace.getState().refreshAll();
    setBusy(null);
    if (done === 1 && last) navigate({ name: "thread", id: last });
    else if (done > 0) { pushToast(`Imported ${done} sessions`); await find(); }
  };
  const addFolder = async (session: HistorySession) => {
    setBusy(keyOf(session));
    try { await api.addProject(session.cwd); await useWorkspace.getState().refreshAll(); await find(); }
    catch (failure) { pushToast(`Couldn't add the folder: ${toFailure(failure).message}`); }
    finally { setBusy(null); }
  };
  const projects = new Set((scan?.sessions ?? []).map((session) => session.cwd)).size;
  return (
    <>
      <BackBar title="Agent history import" onBack={onBack} />
      {scan === null ? (busy === "scan" ? <Loading label="Looking for sessions" rows={4} /> : null) : scan.sessions.length === 0 ? (
        <Empty title="No sessions found"><p>Claude Code and Codex haven't kept any sessions on this computer.</p><Button small onClick={() => void find()}>Scan again</Button></Empty>
      ) : (
        <>
          <div className="set-licence-bar">
            <span className="t3">Found {scan.sessions.length} sessions in {projects} projects</span>
            <span className="set-back-end">
              <Button small disabled={busy !== null} onClick={() => setPicked(new Set(available.map(keyOf)))}>Select all</Button>
              <Button small disabled={busy !== null} onClick={() => void find()}>Scan again</Button>
            </span>
          </div>
          <div className="set-tbl" role="table" aria-label="Sessions">
            <div className="set-tr h" style={{ gridTemplateColumns: "24px minmax(0, 1fr) 150px 110px" }}><span /><span>Session</span><span>Project</span><span>Last used</span></div>
            {scan.sessions.map((session) => {
              const key = keyOf(session);
              const can = session.importable && !session.thread_id;
              return (
                <div key={key} role="row" className="set-tr" style={{ gridTemplateColumns: "24px minmax(0, 1fr) 150px 110px" }}>
                  <span><input type="checkbox" aria-label={`Import ${session.title}`} disabled={!can || busy !== null} checked={picked.has(key)}
                    onChange={(event) => { const next = new Set(picked); if (event.target.checked) next.add(key); else next.delete(key); setPicked(next); }} /></span>
                  <span>{session.title}</span>
                  <span className="t3" title={session.cwd}>
                    {session.thread_id ? <Button small tone="quiet" onClick={() => navigate({ name: "thread", id: session.thread_id ?? "" })}>Open</Button>
                      : !session.importable ? (session.project_id
                        ? <Button small tone="quiet" onClick={() => navigate({ name: "trust", projectId: session.project_id ?? "", threadId: null })}>Trust folder</Button>
                        : <Button small tone="quiet" disabled={busy !== null} onClick={() => void addFolder(session)}>Add folder</Button>)
                      : folderName(session.cwd)}
                  </span>
                  <span className="t3">{day(session.updated_at)}</span>
                </div>
              );
            })}
          </div>
          {scan.truncated ? <p className="setting-line-desc">Only the newest conversations are listed.</p> : null}
          <div className="set-tools set-tools-end">
            <Button tone="primary" busy={busy === "import"} disabled={picked.size === 0 || busy !== null} onClick={() => void importPicked()}>
              Import {picked.size} {picked.size === 1 ? "session" : "sessions"}
            </Button>
          </div>
        </>
      )}
    </>
  );
}
