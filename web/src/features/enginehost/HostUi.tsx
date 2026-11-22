import { CircleAlert, Info, Loader2 } from "lucide-react";
import { useRef, useState } from "react";
import { api } from "../../api/client";
import { toFailure } from "../../lib/useAsync";
import { navigate } from "../../product/state/route";
import { useWorkspace } from "../../product/state/workspace";
import { Button } from "../../ui/controls";
import type { HostRow, HostState } from "./contract";
import "./enginehost.css";
import { cannotSend, type Gap, type GapAction } from "./gaps";
import { engineHostApi, hostWritable, useEngineHosts } from "./hostApi";
import { cantStartLead, mismatchLine, plain } from "./hostText";
import { isClaudeEngine, useClaudeEnginePath } from "../../product/engines/claudePath";

/** The host row for an engine, or null while the service reports none. */
export function useHostFor(engineId: string | null): HostRow | null {
  const { hosts } = useEngineHosts();
  return (engineId ? hosts?.find((row) => row.id === engineId) : null) ?? null;
}

/** One write per click: a second click while the first request is in flight is not another request. */
function useOnce(run: () => Promise<unknown>) {
  const pending = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const click = async () => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError(null);
    try { await run(); }
    catch (failure) { setError(plain(failure instanceof Error && failure.message ? failure.message : toFailure(failure).message, 300)); }
    finally { pending.current = false; setBusy(false); }
  };
  return { busy, error, click: () => void click() };
}

/** The downloaded app update, when one is ready to apply; null when there is nothing to offer. */
export function useAppUpdateOrbyn(): (() => Promise<void>) | null {
  const app = useWorkspace((state) => state.updates?.data?.app);
  if (!app || app.phase !== "ready" || !app.can_apply || !app.release_id) return null;
  const release = app.release_id;
  const operation = app.operation ?? 0;
  return () => api.applyAppUpdate({ release_id: release, operation });
}

const LABELS: Partial<Record<GapAction, string>> = { try_again: "Try again", update: "Update Orbyn", sign_in_again: "Sign in again", check_again: "Check again" };

/** The one live part of a picker row: Try again, Update Orbyn, Sign in again or Check again. Nothing else in the row can be pressed. */
function GapButton({ gap, host }: { gap: Gap; host: HostRow | null }) {
  const update = useAppUpdateOrbyn();
  const action = gap.action;
  const once = useOnce(async () => {
    if (action === "try_again" && host) await engineHostApi().retry(host.id);
    else if (action === "update" && update) await update();
    else if (action === "sign_in_again") navigate({ name: "settings", section: "providers" });
    else if (action === "check_again") {
      await api.refreshEngines();
      await useWorkspace.getState().refreshEngines();
    }
  });
  if (!action || !LABELS[action]) return null;
  // Update Orbyn is offered only when the app has an update to apply; with none the row says what is wrong and nothing more.
  if (action === "update" && !update) return null;
  return (
    <>
      <Button small busy={once.busy} onClick={once.click}>{LABELS[action]}</Button>
      {once.error ? <span className="field-error" role="alert">{once.error}</span> : null}
    </>
  );
}

/**
 * An engine's row in the engine picker when its host is not ready: in place of its models, in the same slot. It is not a
 * model, so nothing in it can be picked, and Try again, Update Orbyn, Sign in again and Check again are its only live parts.
 */
export function PickerGapRow({ gap, host, showChip = true }: { gap: Gap; host: HostRow | null; showChip?: boolean }) {
  const chip = gap.chip;
  return (
    <div className="eh-gap" data-gap={gap.kind}>
      <span className="eh-gap-text">
        <b>{gap.title}</b>
        <small>{gap.sentence}</small>
      </span>
      <span className="eh-gap-end">
        {showChip ? <span className={chip.tone === "plain" ? "set-pill dot" : `set-pill ${chip.tone}${chip.spin ? "" : " dot"}`}>
          {chip.spin ? <Loader2 size={11} className="spin" aria-hidden /> : null}{chip.label}
        </span> : null}
        <GapButton gap={gap} host={host} />
      </span>
    </div>
  );
}

/** The host states that stop the Claude SDK path. A host that is only idle or starting says nothing. */
const CLAUDE_HOST_DOWN = new Set<HostState>(["crashed", "restarting", "cant_start", "version_mismatch"]);

/** True for a host state that stops the Claude SDK path. */
export const claudeHostDown = (host: HostRow | null): boolean => host !== null && CLAUDE_HOST_DOWN.has(host.state);

/** True while a down host is coming back by itself, which is drawn blue rather than red. */
export const claudeHostWaiting = (host: HostRow): boolean => host.state === "restarting" || host.state === "crashed";

/**
 * The service's own sentence for a host that stops the Claude SDK path; the built host lines speak when an older service
 * sends no detail. It is the same sentence the Providers page shows, so the picker and the thread never invent one.
 */
export function claudeHostSentence(host: HostRow): string | null {
  if (!CLAUDE_HOST_DOWN.has(host.state)) return null;
  if (host.detail) return host.detail;
  if (claudeHostWaiting(host)) return "The engine host stopped and is restarting.";
  if (host.state === "version_mismatch") return mismatchLine(host);
  return `${cantStartLead(host)}${host.last_error ? ` Last error: ${plain(host.last_error.message, 200).replace(/\.$/, "")}.` : ""}`;
}

/**
 * The two ways out of a Claude host that is down: Use Claude (CLI) writes the app-only setting, Try again asks the host to
 * start only where it gave up, and Update Orbyn only when an update is ready. One write at a time. Nothing switches by itself.
 */
export function ClaudeHostActions({
  host,
  hostId,
  onError,
  showUpdate = true,
  alwaysRetry = false,
}: {
  host: HostRow | null;
  /** The host to start; the row's own id when one was reported. */
  hostId?: string | null;
  onError(message: string): void;
  showUpdate?: boolean;
  /** A failed run keeps Try again even though no host state is reported. */
  alwaysRetry?: boolean;
}) {
  const { path, choose } = useClaudeEnginePath();
  const update = useAppUpdateOrbyn();
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const run = async (prefix: string, work: () => Promise<unknown>) => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    try {
      await work();
    } catch (failure) {
      onError(`${prefix} ${plain(toFailure(failure).message, 300)}`);
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };
  const retryId = hostId ?? host?.id ?? null;
  const retry = alwaysRetry || (host?.state === "cant_start" && host.can_retry === true);
  return (
    <>
      {showUpdate && host?.state === "version_mismatch" && update ? (
        <Button small busy={busy} onClick={() => void run("That didn’t work.", update)}>Update Orbyn</Button>
      ) : null}
      {path === "sdk" ? (
        <Button small busy={busy} onClick={() => void run("Nothing changed.", async () => {
          const result = await choose("cli");
          if (!result.saved) throw new Error(result.error ?? "The Claude path setting is not available yet.");
        })}>Use Claude (CLI)</Button>
      ) : null}
      {retry && retryId ? (
        <Button small busy={busy} onClick={() => void run("That didn’t work.", () => engineHostApi().retry(retryId))}>Try again</Button>
      ) : null}
    </>
  );
}

/**
 * What the composer says and does while an engine's host is unavailable. For Claude under the SDK path every host-down
 * state gets the service's sentence and its actions; other engines keep their own wording.
 */
export function useHostLine(engineId: string | null, engineName: string, instanceOf?: string | null): { line: HostLine | null; sendBlocked: boolean } {
  const host = useHostFor(instanceOf ?? engineId);
  if (!host) return { line: null, sendBlocked: false };
  if (isClaudeEngine(engineId ?? "", instanceOf)) {
    if (!claudeHostDown(host)) return { line: null, sendBlocked: false };
    // Send is never redirected and the SDK path never falls back: the run fails with its own sentence instead.
    return { line: { host, engineName: plain(engineName, 60), claude: true }, sendBlocked: false };
  }
  if (host.state !== "cant_start") return { line: null, sendBlocked: false };
  return { line: { host, engineName: plain(engineName, 60) }, sendBlocked: cannotSend(host) };
}

export interface HostLine { host: HostRow; engineName: string; claude?: boolean }

/** The host states where the SDK path can still run; anything else is not "fine" and gets no quiet switch. */
const HEALTHY_HOST = new Set<HostState>(["not_started", "starting", "running", "stopped"]);

/**
 * The one quiet line above the composer when the person chose Claude (CLI) and the engine host is fine: the choice is
 * theirs and nothing is wrong, so this only offers the way back. It writes the app-only setting for the next message;
 * there is no notice and no dialog, and it is absent for every other engine and for a host that is down.
 */
export function ClaudePathLine({ engineId, instanceOf }: { engineId: string; instanceOf?: string | null }) {
  const { path, loaded, choose } = useClaudeEnginePath();
  const host = useHostFor(instanceOf ?? engineId);
  const [switching, setSwitching] = useState(false);
  if (!loaded || path !== "cli" || !isClaudeEngine(engineId, instanceOf) || !host || !HEALTHY_HOST.has(host.state)) return null;
  const switchToSdk = async () => {
    if (switching) return;
    setSwitching(true);
    await choose("sdk");
    setSwitching(false);
  };
  return (
    <div className="eh-cli-line">
      <span>Claude (CLI) · </span>
      <button type="button" className="eh-cli-switch" disabled={switching} onClick={() => void switchToSdk()}>Switch to Claude (SDK)</button>
    </div>
  );
}

/**
 * The line above the composer while a host is down. For Claude under the SDK path it is the service's sentence with Use
 * Claude (CLI) and the state's own action; every other engine says it can't send and offers Restart engine host. Either way
 * it stays until the host is back, and the message the person typed stays in the box.
 */
export function ComposerHostLine({ line }: { line: HostLine }) {
  return line.claude ? <ClaudeComposerHostLine line={line} /> : <EngineComposerHostLine line={line} />;
}

/** Claude under the SDK path: the service's sentence, Use Claude (CLI), and the action its host state allows. */
function ClaudeComposerHostLine({ line }: { line: HostLine }) {
  const { path } = useClaudeEnginePath();
  const [error, setError] = useState<string | null>(null);
  const sentence = claudeHostSentence(line.host);
  if (path !== "sdk" || !sentence) return null;
  const waiting = claudeHostWaiting(line.host);
  const Icon = waiting ? Info : CircleAlert;
  return (
    <div className={`eh-line${waiting ? " info" : ""}`} role="alert" data-send="on">
      <Icon size={16} aria-hidden />
      <span>{sentence}{error ? ` ${error}` : ""}</span>
      <ClaudeHostActions host={line.host} onError={setError} />
    </div>
  );
}

/** Every other engine: its run can't reach the host, and a wrapped driver is not a fallback, so Send stays off. */
function EngineComposerHostLine({ line }: { line: HostLine }) {
  const { host, engineName } = line;
  const once = useOnce(() => engineHostApi().retry(host.id));
  const last = host.last_error ? ` Last error: ${plain(host.last_error.message, 200).replace(/\.$/, "")}.` : "";
  const text = `${engineName} can’t send: its engine host is unavailable and ${engineName} has no other way to run here.${last}`;
  return (
    <div className="eh-line" role="alert" data-send="off">
      <CircleAlert size={16} aria-hidden />
      <span>{text}{once.error ? ` ${once.error}` : ""}</span>
      {hostWritable() ? <Button small busy={once.busy} onClick={once.click}>Restart engine host</Button> : null}
    </div>
  );
}
