import { Check, CloudOff, Settings } from "lucide-react";
import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import { navigate } from "../../product/state/route";
import { IconButton } from "../../ui/controls";
import { usePopover } from "../../ui/usePopover";
import type { AccountView } from "../../generated/AccountView";
import { accountApi } from "./api";
import { pauseInfo } from "./pauseCopy";
import { lastSynced, syncLine } from "./syncCopy";
import { useAccount } from "./useAccount";
import "./account.css";

/** The account while it is signed in on this computer; null otherwise, so nothing of it shows. */
export function useSignedInAccount(): { view: AccountView; reload(): void } | null {
  const account = useAccount();
  const view = account.data;
  return view?.available && view.session ? { view, reload: account.reload } : null;
}

/** Whether the Settings list shows the Account category: only when the gate is on and a backend answers. */
export function useAccountAvailable(): boolean {
  return useAccount().data?.available === true;
}

const changes = (count: number) => `${count} ${count === 1 ? "change" : "changes"}`;

function Status({ view }: { view: AccountView }) {
  const line = syncLine(view.sync);
  return (
    <span className="ac-st" data-tone={line.tone}>
      {line.done ? <Check size={12} aria-hidden /> : line.tone === "bad" ? <span className="ac-d" aria-hidden /> : view.sync.kind === "offline" ? <CloudOff size={12} aria-hidden /> : null}
      {line.label}
    </span>
  );
}

function SyncPopover({ view, anchor, place, reload, onClose }: { view: AccountView; anchor: HTMLElement; place: "above" | "below"; reload(): void; onClose(): void }) {
  const root = useRef<HTMLDivElement>(null);
  usePopover({ kind: "fields", root, trigger: () => anchor, onClose });
  const [busy, setBusy] = useState(false);
  const sync = view.sync;
  const rect = anchor.getBoundingClientRect();
  const style =
    place === "above"
      ? { left: Math.max(8, rect.left), bottom: window.innerHeight - rect.top + 8 }
      : { right: Math.max(8, window.innerWidth - rect.right), top: rect.bottom + 8 };
  const refused = sync.kind === "paused" && (sync.reason === "zone_failed" || sync.reason === "too_large");
  const paused = sync.kind === "paused" && !refused;
  const pause = paused ? pauseInfo(sync) : null;
  const offline = sync.kind === "offline";
  const last = lastSynced(sync);
  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    try {
      await work();
      reload();
    } catch {
      // The status says what is true; a failed request changes nothing.
    } finally {
      setBusy(false);
    }
  };

  let title = "Up to date";
  let detail = "Your settings are the same on every signed-in computer.";
  if (refused) {
    title = "Couldn’t sync";
    detail = `The server didn’t accept ${changes(Math.max(1, sync.waiting))}. Nothing was lost; it is kept here.`;
  } else if (offline) {
    title = "Offline";
    detail = `No connection. ${changes(sync.waiting)} ${sync.waiting === 1 ? "is" : "are"} saved on this computer and will send when you’re back online.`;
  } else if (paused) {
    title = "Paused";
    detail = pause?.detail ?? "Sync is paused.";
  } else if (sync.kind !== "online" || sync.waiting > 0) {
    title = "Syncing…";
    detail = "Sending or receiving your settings.";
  }
  return createPortal(
    <div ref={root} className="ac-pop" role="dialog" aria-label="Sync" style={style}>
      <h2>Sync</h2>
      <div className="ac-big">{title === "Up to date" ? <Check size={14} aria-hidden /> : null}{title}</div>
      <p className="ac-sub">{detail}</p>
      <div className="ac-zone">
        <b>Settings</b>
        <span>{sync.waiting} waiting</span>
        <small>
          {refused ? `${changes(Math.max(1, sync.waiting))} couldn’t be sent${last ? ` · last synced ${last}` : ""}` : last ? `Last synced ${last}` : "Not synced yet"}
        </small>
      </div>
      <div className="ac-acts">
        {pause ? (
          pause.action.kind === "resume" ? (
            <button type="button" className="btn btn-secondary btn-small" disabled={busy || !pause.action.enabled} onClick={() => void act(() => accountApi.resumeSync())}>Resume sync</button>
          ) : pause.action.kind === "keybindings" ? (
            <button type="button" className="btn btn-secondary btn-small" onClick={() => { onClose(); navigate({ name: "settings", section: "keybindings" }); }}>Open keybindings</button>
          ) : pause.action.kind === "retry" ? (
            <button type="button" className="btn btn-secondary btn-small" disabled={busy} onClick={() => void act(() => accountApi.syncNow())}>Try again</button>
          ) : null
        ) : (
          <>
            <button type="button" className="btn btn-secondary btn-small" disabled={busy || offline} onClick={() => void act(() => accountApi.syncNow())}>
              {refused ? "Try again" : "Sync now"}
            </button>
            <button type="button" className="btn btn-secondary btn-small" disabled={busy} onClick={() => void act(() => accountApi.pauseSync())}>Pause sync</button>
          </>
        )}
      </div>
      {pause?.off ? <p className="ac-sub">{pause.off}</p> : null}
      <button type="button" className="ac-lnk" onClick={() => { onClose(); navigate({ name: "settings", section: "account" }); }}>Account settings</button>
    </div>,
    document.body,
  );
}

/** The sidebar footer while signed in: avatar and sync status that opens the popover, and the Settings gear. */
export function SyncFooter({ view, reload, settingsCurrent }: { view: AccountView; reload(): void; settingsCurrent: boolean }) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const line = syncLine(view.sync);
  const email = view.session?.email ?? "";
  return (
    <div className="ac-foot">
      <button
        ref={button}
        type="button"
        className="ac-me"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`Account and sync: ${line.label}`}
        data-on={open || undefined}
        onClick={() => setOpen((was) => !was)}
      >
        <span className="ac-avs" aria-hidden>{email.slice(0, 1).toUpperCase()}</span>
        <Status view={view} />
      </button>
      <IconButton label="Settings" pressed={settingsCurrent} onClick={() => navigate({ name: "settings", section: null })}>
        <Settings size={15} aria-hidden />
      </IconButton>
      {open && button.current ? <SyncPopover view={view} anchor={button.current} place="above" reload={reload} onClose={() => setOpen(false)} /> : null}
    </div>
  );
}

/** Below the width where the sidebar folds away its footer is hidden too: one icon in the title bar, in the same tone. */
export function SyncTitleButton({ view, reload }: { view: AccountView; reload(): void }) {
  const [open, setOpen] = useState(false);
  const button = useRef<HTMLButtonElement>(null);
  const line = syncLine(view.sync);
  return (
    <>
      <button
        ref={button}
        type="button"
        className="tbi ac-tbi"
        data-tone={line.tone}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`Sync: ${line.label}`}
        onClick={() => setOpen((was) => !was)}
      >
        {line.done ? <Check size={16} aria-hidden /> : line.tone === "bad" ? <span className="ac-d" aria-hidden /> : view.sync.kind === "offline" ? <CloudOff size={16} aria-hidden /> : <span className="ac-spin" aria-hidden />}
      </button>
      {open && button.current ? <SyncPopover view={view} anchor={button.current} place="below" reload={reload} onClose={() => setOpen(false)} /> : null}
    </>
  );
}
