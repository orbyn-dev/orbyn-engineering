import { Check, Info, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { addRegistryEntry, api, engineInstalls } from "../api";
import type { EngineStatus } from "../../../api/types";
import type { InstallOutcome } from "../../../generated/InstallOutcome";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useWorkspace } from "../../../product/state/workspace";
import { Button, LinkButton } from "../../../ui/controls";
import { SettingRow } from "../rows/SettingRow";
import { SettingsBanner } from "../rows/SettingsBanner";
import { useEngineUpdates } from "./engineUpdates";

/** A finished update is announced for this long; a failed one stays until the next attempt. */
const DONE_SHOWN_MS = 10 * 60 * 1000;

/** The seconds an attempt took, at least one: "in 4 s" reads better than "in 0 s". */
export const tookSeconds = (outcome: InstallOutcome) =>
  Math.max(1, Math.round((Date.parse(outcome.finished_at) - Date.parse(outcome.started_at)) / 1000));

/** The kept outcomes of one engine and what the page should show of them. */
export function useInstallView(engine: EngineStatus) {
  const installs = useAsync(`engine-installs:${engine.id}`, (signal) => engineInstalls(engine.id, signal), { keep: true });
  const live = useEngineUpdates((state) => state.logs[engine.id]);
  const ended = useEngineUpdates((state) => state.ended[engine.id]);
  const running = useEngineUpdates((state) => engine.id in state.running) || engine.updating;
  const { reload } = installs;
  // An update that just ended is read again, so its outcome shows without a reload of the page.
  useEffect(() => { if (ended) reload(); }, [ended, reload]);
  const latest = installs.data?.outcomes.find((outcome) => outcome.kind !== "remove") ?? null;
  const fresh = latest !== null && (!latest.ok || Date.now() - Date.parse(latest.finished_at) < DONE_SHOWN_MS);
  const shown = running ? null : fresh ? latest : null;
  return { running, shown, live: running ? live : undefined, vendorPage: installs.data?.vendor_page ?? null };
}

export type InstallView = ReturnType<typeof useInstallView>;

/** The command and the lines it printed. The last line is red when the attempt failed. */
export function InstallLog({ command, lines, failed }: { command: string | null; lines: string[]; failed?: boolean }) {
  if (!command && lines.length === 0) return null;
  return (
    <div className="pa-log" role="log" aria-label="Installer output">
      {command ? <span><b>$</b> {command}</span> : null}
      {lines.map((line, at) => <span key={`${at}:${line}`} className={failed && at === lines.length - 1 ? "bad" : undefined}>{line}</span>)}
    </div>
  );
}

/** What was copied for a person to send along: the reason, the command and the lines, all already hidden by the service. */
const details = (outcome: InstallOutcome) =>
  [outcome.reason, outcome.command ? `$ ${outcome.command}` : null, ...outcome.output].filter(Boolean).join("\n");

/** The banner above the version row: the failure with its way forward, or the finished update. */
export function InstallBanner({ engine, view }: { engine: EngineStatus; view: InstallView }) {
  const refreshEngines = useWorkspace((state) => state.refreshEngines);
  const [again, setAgain] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const outcome = view.shown;
  if (!outcome) return null;
  if (outcome.ok) {
    return (
      <SettingsBanner tone="info" icon={<Check size={16} aria-hidden />}>
        {outcome.kind === "install" ? "Installed" : "Updated to"} {outcome.to_version ?? engine.version ?? ""} in {tookSeconds(outcome)} s.
      </SettingsBanner>
    );
  }
  const retry = async () => {
    setAgain(true);
    setError(null);
    try {
      // A registry agent is installed again through the registry, not as a recommended version.
      if (outcome.kind === "install" && engine.id.startsWith("acp-")) await addRegistryEntry(engine.id.slice(4), undefined);
      else if (outcome.kind === "install") await api.installRecommended(engine.id);
      else await api.updateEngine(engine.id);
      await refreshEngines();
    } catch (failure) { setError(toFailure(failure).message); } finally { setAgain(false); }
  };
  const copy = async () => {
    try { await navigator.clipboard.writeText(details(outcome)); setCopied(true); setTimeout(() => setCopied(false), 1500); } catch { setCopied(false); }
  };
  return (
    <>
      <SettingsBanner tone="err" icon={<TriangleAlert size={16} aria-hidden />}
        actions={<><Button small busy={again} onClick={() => void retry()}>Try again</Button><Button small onClick={() => void copy()}>{copied ? "Copied" : "Copy details"}</Button></>}>
        Couldn’t {outcome.kind === "install" ? "install" : "update"} {engine.name}.{outcome.from_version ? ` It is still on ${outcome.from_version}.` : ""} {outcome.reason}
      </SettingsBanner>
      {error ? <p className="field-error" role="alert">{error}</p> : null}
    </>
  );
}

/** The log under the version row: live while the installer runs, then what the service kept. */
export function InstallBody({ view }: { view: InstallView }) {
  if (view.running) {
    return (
      <>
        {view.live ? <InstallLog command={view.live.command} lines={view.live.lines} /> : null}
        <p className="pa-cap">An update can’t be stopped once it starts: stopping an installer halfway could leave the engine half-written. You can close this page; it keeps going.</p>
      </>
    );
  }
  const outcome = view.shown;
  return outcome ? <InstallLog command={outcome.command ?? null} lines={outcome.output} failed={!outcome.ok} /> : null;
}

/**
 * An engine that is not installed. Orbyn installs no native engine itself: before the
 * program exists no installer provably owns it, so the card links to the vendor's own page.
 */
export function VendorInstall({ engine, view, checking, onCheck }: { engine: EngineStatus; view: InstallView; checking: boolean; onCheck(): void }) {
  const page = view.vendorPage;
  // "Orbyn looks again when you come back": returning to the window checks once.
  useEffect(() => {
    const back = () => { if (!checking) onCheck(); };
    window.addEventListener("focus", back);
    return () => window.removeEventListener("focus", back);
  }, [checking, onCheck]);
  return (
    <>
      <SettingsBanner icon={<Info size={16} aria-hidden />} tone="info">{engine.name} isn’t installed on this computer.</SettingsBanner>
      <SettingRow title="Install"
        description={page
          ? `Follow ${engine.name}’s own instructions on its official install page, ${page.domain}. Orbyn installs nothing itself.`
          : `Follow ${engine.name}’s own install instructions. Orbyn installs nothing itself.`}>
        {page ? <LinkButton small href={page.url} target="_blank" rel="noreferrer noopener">Open {engine.name}’s instructions · {page.domain}</LinkButton> : null}
      </SettingRow>
      <SettingRow title="After installing" description="Orbyn looks again when you come back, or on demand.">
        <Button small busy={checking} onClick={onCheck}>Check again</Button>
      </SettingRow>
    </>
  );
}
