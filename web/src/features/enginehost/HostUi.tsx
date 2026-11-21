import { CircleAlert, Loader2 } from "lucide-react";
import { useRef, useState } from "react";
import { api } from "../../api/client";
import { toFailure } from "../../lib/useAsync";
import { navigate } from "../../product/state/route";
import { useWorkspace } from "../../product/state/workspace";
import { Button } from "../../ui/controls";
import type { HostRow } from "./contract";
import "./enginehost.css";
import { cannotSend, sendsWithoutHost, type Gap, type GapAction } from "./gaps";
import { engineHostApi, useEngineHosts } from "./hostApi";
import { plain } from "./hostText";

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
function useUpdateOrbyn(): (() => Promise<void>) | null {
  const app = useWorkspace((state) => state.updates.data?.app);
  if (!app || app.phase !== "ready" || !app.can_apply || !app.release_id) return null;
  const release = app.release_id;
  const operation = app.operation ?? 0;
  return () => api.applyAppUpdate({ release_id: release, operation });
}

const LABELS: Partial<Record<GapAction, string>> = { try_again: "Try again", update: "Update Orbyn", sign_in_again: "Sign in again" };

/** The one live part of a picker row: Try again, Update Orbyn or Sign in again. Nothing else in the row can be pressed. */
function GapButton({ gap, host }: { gap: Gap; host: HostRow | null }) {
  const update = useUpdateOrbyn();
  const action = gap.action;
  const once = useOnce(async () => {
    if (action === "try_again" && host) await engineHostApi().retry(host.id);
    else if (action === "update" && update) await update();
    else if (action === "sign_in_again") navigate({ name: "settings", section: "providers" });
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
 * model, so nothing in it can be picked, and Try again, Update Orbyn and Sign in again are its only live parts.
 */
export function PickerGapRow({ gap, host }: { gap: Gap; host: HostRow | null }) {
  const chip = gap.chip;
  return (
    <div className="eh-gap" data-gap={gap.kind}>
      <span className="eh-gap-text">
        <b>{gap.title}</b>
        <small>{gap.sentence}</small>
      </span>
      <span className="eh-gap-end">
        <span className={chip.tone === "plain" ? "set-pill dot" : `set-pill ${chip.tone}${chip.spin ? "" : " dot"}`}>
          {chip.spin ? <Loader2 size={11} className="spin" aria-hidden /> : null}{chip.label}
        </span>
        <GapButton gap={gap} host={host} />
      </span>
    </div>
  );
}

/** What the composer says and does while an engine's host is unavailable. */
export function useHostLine(engineId: string | null, engineName: string): { line: HostLine | null; sendBlocked: boolean } {
  const host = useHostFor(engineId);
  if (!host || host.state !== "cant_start") return { line: null, sendBlocked: false };
  return { line: { host, engineName: plain(engineName, 60), fallback: sendsWithoutHost(host) }, sendBlocked: cannotSend(host) };
}

export interface HostLine { host: HostRow; engineName: string; fallback: boolean }

/**
 * The red line above the composer after five restarts in five minutes. With a Rust driver Send still works (the run falls back to
 * it); without one Send is off and the line carries Restart engine host. Either way it stays until the host is back, and the
 * message the person typed stays in the box.
 */
export function ComposerHostLine({ line }: { line: HostLine }) {
  const { host, engineName, fallback } = line;
  const once = useOnce(() => engineHostApi().retry(host.id));
  const last = host.last_error ? ` Last error: ${plain(host.last_error.message, 200).replace(/\.$/, "")}.` : "";
  const text = fallback
    ? `${engineName} can’t start: its engine host stopped ${host.stops_in_window ?? 5} times in 5 minutes.${last}`
    : `${engineName} can’t send: its engine host is unavailable and ${engineName} has no other way to run here.${last}`;
  return (
    <div className="eh-line" role="alert" data-send={fallback ? "falls-back" : "off"}>
      <CircleAlert size={16} aria-hidden />
      <span>{text}{once.error ? ` ${once.error}` : ""}</span>
      <Button small busy={once.busy} onClick={once.click}>{fallback ? "Try again" : "Restart engine host"}</Button>
    </div>
  );
}
