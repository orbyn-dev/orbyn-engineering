import { Bell, Eye, Info, MapPin, Shield, Sun, Type, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useAsync } from "../../../lib/useAsync";
import { IconButton, Pill } from "../../../ui/controls";
import { useEscapeClose } from "../../../ui/useEscapeClose";
import type { Device } from "../contract";
import { plain } from "../model";
import { devicesPlannedApi, usePlannedApiVersion, type DeviceToolChange, type PushOutcome } from "../planned";
import { DeviceToolsPlatforms } from "./DeviceToolsPlatforms";
import { AccessibilityPane, AppearancePane, LocationPane, PermissionsPane, PushPaneFor, TextPane } from "./ToolsContent";
import "./tools.css";

export type ToolTab = "appearance" | "text" | "accessibility" | "location" | "permissions" | "push";

const TABS: readonly { id: ToolTab; label: string; icon: ReactNode }[] = [
  { id: "appearance", label: "Appearance", icon: <Sun size={14} aria-hidden /> },
  { id: "text", label: "Text", icon: <Type size={14} aria-hidden /> },
  { id: "accessibility", label: "Accessibility", icon: <Eye size={14} aria-hidden /> },
  { id: "location", label: "Location", icon: <MapPin size={14} aria-hidden /> },
  { id: "permissions", label: "Permissions", icon: <Shield size={14} aria-hidden /> },
  { id: "push", label: "Push", icon: <Bell size={14} aria-hidden /> },
];

const labelOf = (tab: ToolTab): string => TABS.find((one) => one.id === tab)?.label ?? "Device tools";

/**
 * The Device tools pill (H26). It is a Planned pill like Live screen and Record, and it opens the drawer over the
 * device panel. The drawer reads the Planned client; in production that answers `null`, so it draws the Planned note
 * and no control. Nothing here touches Orbyn's own appearance.
 */
export function DeviceToolsButton({ device, live }: { device: Device; live: boolean }) {
  const [open, setOpen] = useState(false);
  const [host, setHost] = useState<HTMLElement | null>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const close = () => setOpen(false);
  useEscapeClose(open, close, () => trigger.current);

  useEffect(() => {
    if (!open) { setHost(null); return; }
    const panel = trigger.current?.closest(".dev");
    setHost(panel instanceof HTMLElement ? panel : document.body);
  }, [open]);

  const drawer = open && host ? <DeviceToolsDrawer device={device} live={live} onClose={close} floating={host === document.body} /> : null;

  return (
    <>
      <button
        ref={trigger}
        type="button"
        className="dev-tools-trigger"
        aria-expanded={open}
        aria-label="Device tools"
        onClick={() => setOpen((held) => !held)}
      >
        Device tools · Planned
      </button>
      {drawer && host ? createPortal(drawer, host) : null}
    </>
  );
}

/**
 * The drawer: the six tabs of the design plus the platform table. Every control is marked Not verified until
 * Engineering checks it against simctl and adb; a control a platform cannot do says so. With no Planned client the
 * body is one Planned note. The drawer never sends anything in production.
 */
export function DeviceToolsDrawer({ device, initialTab = "appearance", onClose, floating = false, live = false }: {
  device: Device;
  initialTab?: ToolTab;
  onClose(): void;
  /** The preview and a standalone DeviceView have no `.dev` panel to sit in; the drawer then floats. */
  floating?: boolean;
  /** The device's live stream is running. Without it, the accessibility outlines stay disabled. */
  live?: boolean;
}) {
  const seen = usePlannedApiVersion();
  const [tab, setTab] = useState<ToolTab>(initialTab);
  const [view, setView] = useState<"tools" | "platforms">("tools");
  const [busy, setBusy] = useState(false);
  const [writeError, setWriteError] = useState("");
  const writing = useRef(false);
  const wasLive = useRef(live);
  const read = useAsync(`device-tools:${seen}:${device.id}`, (signal) => devicesPlannedApi().deviceTools(device.id, signal));
  const state = read.status === "ready" ? read.data : null;
  const planned = read.status === "ready" && state === null;
  const reading = read.status === "loading";
  const setData = read.setData;

  useEffect(() => { setTab(initialTab); }, [initialTab, device.id]);

  const apply = useCallback(async (change: DeviceToolChange) => {
    if (writing.current || !state) return;
    writing.current = true;
    setBusy(true);
    setWriteError("");
    try {
      const next = await devicesPlannedApi().setDeviceTool(device.id, change);
      if (next === null) setWriteError("Changing the device’s tools is Planned. The service has no such route yet.");
      else setData(next);
    } catch (error) {
      setWriteError(error instanceof Error ? plain(error.message, 200) : "Couldn’t change that setting.");
    } finally {
      writing.current = false;
      setBusy(false);
    }
  }, [device.id, setData, state]);

  // Outlines belong to the live screen: when the stream goes away, the reader's switch goes off once.
  useEffect(() => {
    if (live) { wasLive.current = true; return; }
    if (!wasLive.current || !state) return;
    wasLive.current = false;
    if (state.accessibility.showElements) void apply({ type: "show_elements", on: false });
  }, [live, state, apply]);

  const sendPush = async (payload: string): Promise<PushOutcome | null> => {
    try {
      return await devicesPlannedApi().sendDevicePush(device.id, payload);
    } catch (error) {
      if (error instanceof Error) return { ok: false, error: plain(error.message, 200) };
      throw error;
    }
  };

  const panes = state ? (
    tab === "appearance" ? <AppearancePane state={state} busy={busy} onChange={(change) => void apply(change)} onSendPush={sendPush} />
    : tab === "text" ? <TextPane state={state} busy={busy} onChange={(change) => void apply(change)} onSendPush={sendPush} />
    : tab === "accessibility" ? <AccessibilityPane state={state} busy={busy} live={live} onChange={(change) => void apply(change)} onSendPush={sendPush} />
    : tab === "location" ? <LocationPane state={state} busy={busy} onChange={(change) => void apply(change)} onSendPush={sendPush} />
    : tab === "permissions" ? <PermissionsPane state={state} busy={busy} onChange={(change) => void apply(change)} onSendPush={sendPush} />
    : <PushPaneFor platform={device.platform} state={state} busy={busy} onChange={(change) => void apply(change)} onSendPush={sendPush} />
  ) : null;

  return (
    <section className={floating ? "dt-drawer dt-drawer-floating" : "dt-drawer"} role="region" aria-label="Device tools">
      <div className="dt-tabs" role="tablist" aria-label="Device tools tabs">
        {view === "tools" ? TABS.map((one) => (
          <button
            key={one.id}
            type="button"
            role="tab"
            className="dt-tab"
            aria-selected={tab === one.id}
            aria-label={one.label}
            title={one.label}
            onClick={() => setTab(one.id)}
          >
            {one.icon}
          </button>
        )) : null}
        <span className="dt-spacer" />
        <b className="dt-name">{view === "platforms" ? "Platforms" : labelOf(tab)}</b>
        {view === "platforms" ? (
          <button type="button" className="dt-back" onClick={() => setView("tools")}>Back to tools</button>
        ) : (
          <IconButton label="What each platform can do" onClick={() => setView("platforms")}><Info size={14} aria-hidden /></IconButton>
        )}
        <IconButton label="Close tools" onClick={onClose}><X size={14} aria-hidden /></IconButton>
      </div>
      <div className="dt-body">
        {view === "platforms" ? <DeviceToolsPlatforms /> : reading ? (
          <p className="dt-note" role="status">Reading the device’s tools…</p>
        ) : read.status === "error" ? (
          <div className="dt-fail" role="alert">
            <span>Could not read the device’s tools. {plain(read.error.message, 200)}</span>
            <button type="button" className="dt-link" onClick={read.reload}>Try again</button>
          </div>
        ) : planned ? (
          <div className="dt-planned" role="status">
            <Pill>Planned</Pill>
            <p>Device tools are Planned. The service has no tools route yet, so Orbyn cannot change this device’s appearance, text, accessibility, location, permissions or push. The Not verified labels stay until that contract exists.</p>
          </div>
        ) : (
          <>
            {panes}
            {writeError ? <p className="dt-error" role="alert">{writeError}</p> : null}
          </>
        )}
      </div>
    </section>
  );
}

/** Re-exported so the preview can draw the table without the drawer chrome. */
export { DeviceToolsPlatforms };

