import { MoreHorizontal, RefreshCw, Smartphone } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { useUi } from "../../product/state/ui";
import { Button, IconButton, Pill } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { devicesApi, useApiVersion, useDevices } from "./api";
import type { Device } from "./contract";
import { DeviceTabs } from "./DeviceTabs";
import { DeviceView } from "./DeviceView";
import { plain, platformLabel, stateLabel } from "./model";
import { usePlannedSupport } from "./planned";
import { SetupHub } from "./setup/SetupHub";
import { ToolVersions } from "./setup/ToolVersions";
import "./devices.css";

type View = "devices" | "setup" | "versions";

const floatPlace = { x: 24, y: 80, width: 360 };

/**
 * The Devices tab (docs/devices.md, Beta, hidden until the service answers). It holds the list the service gave,
 * the setup hub when no platform is available, per-device tabs (the person's labels live in the UI preferences),
 * the one device's stage and toolbar, and the Planned tool-version page. Device ids come only from the list; the
 * web never types one. Every action is one Person-class write, one at a time. The live screen, typing and the tools
 * drawer are Planned (phase 2/4) and drawn as pills.
 */
export function DevicesPanel({ threadId = "" }: { threadId?: string }) {
  const seen = useApiVersion();
  const { list, error: listError, settled, reload } = useDevices();
  const stored = useUi((state) => state.devices);
  const setPrefs = useUi((state) => state.setPrefs);
  const [view, setView] = useState<View | null>(null);
  const [overrides, setOverrides] = useState<ReadonlyMap<string, Device>>(new Map());
  const [gone, setGone] = useState<ReadonlySet<string>>(new Set());
  const [notice, setNotice] = useState("");
  const [failure, setFailure] = useState("");
  const [busy, setBusy] = useState(false);
  const writing = useRef(false);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const setupSupported = usePlannedSupport();
  const [showList, setShowList] = useState(false);

  const devices = (list?.devices ?? []).filter((device) => !gone.has(device.id)).map((device) => overrides.get(device.id) ?? device);
  const platforms = list?.platforms ?? [];
  const anyAvailable = platforms.length === 0 || platforms.some((one) => one.available);
  const active = stored.active && devices.some((one) => one.id === stored.active) ? stored.active : null;
  const device = devices.find((one) => one.id === active) ?? null;
  const open = stored.open.filter((id) => devices.some((one) => one.id === id));

  // The first time the tab opens the hub, or the list when a platform is usable. The hub needs the Planned setup
  // client; without it the list keeps the platform sentences the contract defines. A person who chose a view keeps it.
  useEffect(() => {
    if (view !== null || list === null || setupSupported === null) return;
    setView(!anyAvailable && setupSupported ? "setup" : "devices");
  }, [view, list, anyAvailable, setupSupported]);

  const save = (next: { open?: string[]; active?: string | null; names?: Record<string, string>; float?: typeof stored.float }) => {
    setPrefs({ devices: { ...stored, ...next } });
  };

  const openDevice = (id: string) => {
    const nextOpen = open.includes(id) ? open : [...open, id].slice(-12);
    save({ open: nextOpen, active: id });
    setShowList(false);
    setFailure("");
    setNotice("");
  };

  const closeDevice = (id: string) => {
    const next = open.filter((one) => one !== id);
    const active = stored.active === id ? next[0] ?? null : stored.active;
    save({ open: next, active, float: stored.float?.id === id ? null : stored.float });
  };

  const rename = (id: string, value: string) => {
    const names = { ...stored.names };
    const label = value.trim().slice(0, 60);
    if (label === "") delete names[id];
    else names[id] = label;
    save({ names });
  };

  const goneDevice = (id: string) => {
    setGone((held) => new Set(held).add(id));
    setOverrides((held) => { const next = new Map(held); next.delete(id); return next; });
    save({ open: open.filter((one) => one !== id), active: open.filter((one) => one !== id)[0] ?? null, float: stored.float?.id === id ? null : stored.float });
    setNotice("That device is gone. The list is up to date.");
    reload();
  };

  const shutdown = async (target: Device) => {
    if (writing.current) return;
    writing.current = true;
    setBusy(true);
    setFailure("");
    try {
      const result = await devicesApi().act(target.id, { type: "power_off" });
      if (result.device && result.device.id === target.id) {
        setOverrides((held) => new Map(held).set(target.id, { ...target, ...result.device }));
      }
    } catch (error) {
      if (error instanceof ApiFailure && error.code === "device_not_found") goneDevice(target.id);
      else setFailure(error instanceof ApiFailure ? plain(error.message, 200) : "Something went wrong.");
    } finally {
      writing.current = false;
      setBusy(false);
    }
  };

  const onChanged = (next: Device) => setOverrides((held) => new Map(held).set(next.id, next));

  if (list === null && !listError) return settled ? null : <section className="dev" aria-busy="true" aria-label="Devices" />;
  if (list === null) return <section className="dev"><p className="field-error" role="alert">{plain(listError?.message, 200)}</p></section>;

  const planned = view === "setup" || view === "versions";
  const menuItems: MenuItem[] = [
    { label: "Setup and tools…", onSelect: () => setView("setup") },
    { label: "Tool versions…", onSelect: () => setView("versions") },
    { label: "Agent access…", disabled: true, title: "The agent-access toggle is Planned.", content: <span className="dev-menu-note">Agent access… <Pill>Planned</Pill></span> },
    { label: "All devices", separatorBefore: true, onSelect: () => { setShowList(true); setFailure(""); } },
  ];

  return (
    <section className="dev" aria-label="Devices">
      <div className="dev-head">
        <div className="dev-head-title">
          <b>Devices</b>
          {planned ? <Pill>Planned</Pill> : <Pill tone="info">Beta</Pill>}
        </div>
        <div className="dev-head-actions">
          {view !== "devices" ? <Button small tone="quiet" onClick={() => setView("devices")}>Back to devices</Button> : null}
          <IconButton label="Refresh devices" onClick={() => { setOverrides(new Map()); setGone(new Set()); setNotice(""); setFailure(""); reload(); }} disabled={busy}>
            <RefreshCw size={14} aria-hidden />
          </IconButton>
          <IconButton
            label="Devices menu"
            pressed={menu !== null}
            aria-haspopup="menu"
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              setMenu(menu ? null : { x: rect.right - 200, y: rect.bottom + 4 });
            }}
          >
            <MoreHorizontal size={14} aria-hidden />
          </IconButton>
        </div>
      </div>
      {menu ? <Presence><Menu x={menu.x} y={menu.y} label="Devices menu" items={menuItems} onClose={() => setMenu(null)} /></Presence> : null}

      {view === "setup" ? (
        <SetupHub
          key={`setup:${seen}:${platforms.map((one) => `${one.platform}:${one.available}`).join(",")}`}
          platforms={platforms}
          deviceToOpen={device ?? devices[0] ?? null}
          onOpen={(id) => { setView("devices"); if (id) openDevice(id); }}
          onClose={() => setView("devices")}
        />
      ) : view === "versions" ? (
        <ToolVersions onBack={() => setView("devices")} />
      ) : (
        <>
          {platforms.filter((one) => !one.available).map((one) => (
            <p className="dev-note" role="status" key={one.platform}><b>{platformLabel(one.platform)}</b> <span>{plain(one.message || "Not available on this computer.", 160)}</span></p>
          ))}
          {notice ? <p className="dev-note" role="status">{notice}</p> : null}
          {failure ? <p className="field-error" role="alert">{failure}</p> : null}
          {open.length > 0 && !showList ? (
            <>
              <DeviceTabs
                devices={devices}
                open={open}
                active={active}
                names={stored.names}
                busy={busy}
                onSelect={openDevice}
                onClose={closeDevice}
                onRename={rename}
                onFloat={(id) => {
                  save({ active: id, float: { id, ...floatPlace } });
                  useUi.getState().closePanel();
                }}
                onSetup={() => setView("setup")}
                onShutdown={(one) => void shutdown(one)}
              />
              {device ? (
                <DeviceView
                  key={device.id}
                  device={device}
                  onChanged={onChanged}
                  onGone={goneDevice}
                  onRestart={() => { setOverrides(new Map()); reload(); }}
                />
              ) : (
                <p className="dev-note">Choose a device.</p>
              )}
            </>
          ) : devices.length === 0 ? (
            <p className="dev-note">No devices are listed.</p>
          ) : (
            <>
              <p className="dev-note">Nothing is selected until a row is chosen.</p>
              <ul className="dev-list" aria-label="Devices">
                {devices.map((one) => (
                  <li key={one.id}>
                    <button type="button" className="dev-row" disabled={busy} onClick={() => openDevice(one.id)}>
                      <Smartphone size={14} aria-hidden />
                      <span className="dev-name">{plain(one.name, 60)}</span>
                      <span className="dev-meta">{`${platformLabel(one.platform)} ${plain(one.version, 16)}`.trim()}</span>
                      {one.foldable ? <span className="pill pill-accent">Foldable</span> : null}
                      <span className="dev-state" data-state={one.state}>{stateLabel(one.state)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </>
      )}
      <span className="sr-only">{threadId ? `Devices for ${threadId}` : "Devices"}</span>
    </section>
  );
}
