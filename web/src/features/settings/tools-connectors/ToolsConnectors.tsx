import { AlertTriangle, RefreshCw } from "lucide-react";
import { useState } from "react";
import { api } from "../api";
import { useAsync } from "../../../lib/useAsync";
import { useWaited } from "../../../lib/motion";
import { useWorkspace } from "../../../product/state/workspace";
import { Button } from "../../../ui/controls";
import { Loading } from "../../../product/ui/states";
import { tabKeys } from "../../../ui/tabKeys";
import { ToolsList } from "../../tools";

const clock = (iso: string | null | undefined) => {
  const date = iso ? new Date(iso) : null;
  return date && !Number.isNaN(date.getTime()) ? date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false }) : null;
};

/** One engine's list. The engine is asked when the tab opens and again on Check again. */
function EnginePanel({ engine, name }: { engine: string; name: string }) {
  const tools = useAsync(`engine-tools:${engine}`, (signal) => api.engineTools(engine, false, signal));
  const [checking, setChecking] = useState(false);
  const waited = useWaited(tools.status === "loading");
  const checkAgain = async () => {
    setChecking(true);
    try {
      const fresh = await api.engineTools(engine, true);
      tools.setData(fresh);
    } catch {
      await tools.reload();
    } finally {
      setChecking(false);
    }
  };
  const retry = () => void (tools.status === "error" ? tools.reload() : checkAgain());
  const data = tools.data;
  const at = clock(data?.checked_at);
  // A check can fail as a request, or come back 200 with an error beside the last good list.
  const failed = tools.status === "error" || Boolean(data?.error);
  return (
    <div role="tabpanel" aria-label={name}>
      <div className="tg-set-head">
        <span className="t3">{at ? `Last checked ${at}` : failed ? "Not checked" : ""}</span>
        <Button small icon={<RefreshCw size={13} className={checking || tools.refreshing ? "spin" : undefined} aria-hidden />} disabled={checking} onClick={() => void checkAgain()}>
          Check again
        </Button>
      </div>
      {tools.status === "loading" ? (
        waited ? <Loading label={`Checking ${name}`} rows={4} /> : <span className="sr-only" role="status">Checking {name}</span>
      ) : null}
      {failed ? (
        <div className="tg-row tg-row-bad tg-fail" role="alert">
          <AlertTriangle size={14} aria-hidden className="tg-icon" />
          <span className="tg-name">
            <b>Couldn’t check {name} tools</b>
            <small className="tg-why">{data?.error ?? `${name} didn’t answer.`}{data ? ` The list below is from ${at ?? "earlier"}.` : ""}</small>
          </span>
          <Button small onClick={retry}>Retry</Button>
        </div>
      ) : null}
      {data ? (
        <div className={failed ? "tg-stale" : undefined} aria-disabled={failed || undefined}>
          <p className="tg-note tg-lead">
            {data.source === "run"
              ? `Read from ${name} when each run starts; projects can differ. Orbyn shows what the engine reports and changes nothing: to turn a tool on or off, use the engine’s own settings.`
              : data.source === "check+run"
                ? `Servers and connectors are read from ${name} when this page opens; plugins, built-in tools and skills when each run starts, so projects can differ. Orbyn shows what the engine reports and changes nothing: to turn a tool on or off, use the engine’s own settings.`
                : `Read from ${name} when this page opens. Orbyn shows what ${name} reports and changes nothing: to turn a tool on or off, use ${name}’s own settings.`}
          </p>
          <ToolsList tools={data} engineName={name} mode="settings" />
        </div>
      ) : null}
    </div>
  );
}

/** Settings › Agents › Tools and connectors: what each engine has, read-only. */
export function ToolsConnectorsSection() {
  // Only Claude Code and Codex can say what they have; the other engines have no such report.
  const engines = (useWorkspace((state) => state.engines.data) ?? []).filter(
    (one) => one.installed && ["claude", "codex"].includes(one.instance_of ?? one.id),
  );
  const [picked, setPicked] = useState<string | null>(null);
  const current = engines.find((one) => one.id === picked) ?? engines[0];
  return (
    <>
      <h1 className="sr-only" tabIndex={-1} data-page-title>
        Tools and connectors
      </h1>
      {current ? (
        <>
          <div role="tablist" aria-label="Engine" className="set-tabs" onKeyDown={(event) => tabKeys(event, (at) => setPicked(engines[at]?.id ?? null))}>
            {engines.map((one) => (
              <button key={one.id} type="button" role="tab" aria-selected={current.id === one.id} tabIndex={current.id === one.id ? 0 : -1} onClick={() => setPicked(one.id)}>
                {one.name}
              </button>
            ))}
          </div>
          <EnginePanel key={current.id} engine={current.id} name={current.name} />
        </>
      ) : (
        <p className="tg-note">Install Claude Code or Codex to see what its threads can use.</p>
      )}
    </>
  );
}
