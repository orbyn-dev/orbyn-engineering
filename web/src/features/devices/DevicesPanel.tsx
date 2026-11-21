import { Camera, ChevronLeft, FoldHorizontal, House, Power, RefreshCw, RotateCw, Smartphone, Square, SlidersHorizontal, UnfoldHorizontal } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { ApiFailure } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import { Button, IconButton } from "../../ui/controls";
import { devicesApi, useApiVersion, useDevices } from "./api";
import { HINGE_MAX, HINGE_MIN, type ButtonName, type Device, type DeviceAction, type HingePreset, type Orientation } from "./contract";
import { clampAngle, controlsFor, hingeLabel, platformLabel, plain, postureLabel, screenshotSrc, stateLabel } from "./model";
import "./devices.css";

const ORIENTATIONS: readonly Orientation[] = ["portrait", "landscape_left", "portrait_upside_down", "landscape_right"];
const PRESETS: readonly { preset: HingePreset; label: string }[] = [{ preset: "closed", label: "Closed" }, { preset: "half_open", label: "Half open" }, { preset: "open", label: "Open" }];
/** What the Engineer frames draw that no code can do yet: live view (H22), Record, typing and the tools drawer (H26). */
const PLANNED = ["Live screen", "Record", "Type text", "Device tools"];

interface Failure { text: string; again(): void }

const readout = (device: Device): string | null => {
  if (!device.fold) return null;
  const { posture, hinge_angle } = device.fold;
  return typeof hinge_angle === "number" ? `${postureLabel(posture)} · ${hingeLabel(hinge_angle)}` : postureLabel(posture);
};

/**
 * The device panel (docs/devices.md, provisional): the list the service gave, the toolbar of the selected device and, for a
 * foldable, 2D fold controls (Fold, Unfold, a hinge slider and three presets). Frames `device-fold-*` (Planned). Hidden entirely when
 * the service has no devices route. Device ids come only from the list; the web never types one. Every action is one Person-class
 * write, one at a time. There is no 3D model (owner ruling).
 */
export function DevicesPanel() {
  const seen = useApiVersion();
  const { list, error: listError, settled, reload } = useDevices();
  const [selected, setSelected] = useState<string | null>(null);
  const [overrides, setOverrides] = useState<ReadonlyMap<string, Device>>(new Map());
  const [gone, setGone] = useState<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [notice, setNotice] = useState("");
  const [shot, setShot] = useState<{ id: string; src: string } | null>(null);
  const [hingeOpen, setHingeOpen] = useState(false);
  const [draft, setDraft] = useState(90);
  const turn = useRef(0);

  const devices = (list?.devices ?? []).filter((device) => !gone.has(device.id)).map((device) => overrides.get(device.id) ?? device);
  const device = devices.find((one) => one.id === selected) ?? null;
  const detail = useAsync(`device:${seen}:${device?.id ?? ""}`, (signal) => (device ? devicesApi().detail(device.id, signal) : Promise.resolve(null)));
  const angle = device?.fold?.hinge_angle ?? null;
  useEffect(() => { setDraft(clampAngle(angle ?? 90)); }, [angle, device?.id]);

  if (list === null && !listError) return settled ? null : <section className="dev" aria-busy="true" aria-label="Devices" />;
  if (list === null) return <section className="dev"><p className="field-error" role="alert">{plain(listError?.message, 200)}</p></section>;

  const refresh = () => { setOverrides(new Map()); setGone(new Set()); setNotice(""); reload(); };

  const run = async (target: Device, body: DeviceAction, verb: string): Promise<void> => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setFailure(null);
    setNotice("");
    const fail = (text: string) => setFailure({ text, again: () => { void run(target, body, verb); } });
    try {
      const result = await devicesApi().act(target.id, body);
      if (typeof result.device === "object" && result.device !== null && result.device.id === target.id) {
        const next = result.device;
        setOverrides((held) => new Map(held).set(target.id, next));
      }
      if (body.type === "screenshot") {
        const src = screenshotSrc(result.image);
        if (src) setShot({ id: target.id, src });
        else fail("The service sent something that is not a usable screenshot.");
      }
      if (body.type === "hinge" || body.type === "hinge_preset" || body.type === "fold" || body.type === "unfold") setHingeOpen(false);
    } catch (error) {
      if (error instanceof ApiFailure && error.code === "device_not_found") {
        setGone((held) => new Set(held).add(target.id));
        setSelected(null);
        setNotice("That device is gone. The list is up to date.");
        reload();
      } else {
        const message = error instanceof ApiFailure ? plain(error.message, 200) : "Something went wrong.";
        fail(`Couldn’t ${verb} ${plain(target.name, 40)}. ${message}`);
      }
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const actions = detail.status === "ready" && detail.data ? detail.data.actions : [];
  const controls = device ? controlsFor(device, Array.isArray(actions) ? actions : []) : null;
  const fold = device?.fold;
  const closed = fold?.posture === "closed";
  const press = (name: ButtonName) => device && void run(device, { type: "button", value: name }, `press ${name} on`);
  const rotate = () => {
    if (!device) return;
    turn.current = (turn.current + 1) % ORIENTATIONS.length;
    void run(device, { type: "rotate", value: ORIENTATIONS[turn.current]! }, "rotate");
  };
  const tool = (label: string, icon: ReactNode, onClick: () => void, extra: { pressed?: boolean } = {}) => (
    <IconButton key={label} label={label} onClick={onClick} disabled={busy} pressed={extra.pressed}>{icon}</IconButton>
  );

  return (
    <section className="dev" aria-label="Devices">
      <div className="dev-head">
        <b>Devices</b>
        <IconButton label="Refresh devices" onClick={refresh} disabled={busy}><RefreshCw size={14} aria-hidden /></IconButton>
      </div>
      {list.platforms.filter((one) => !one.available).map((one) => (
        <p className="dev-note" role="status" key={one.platform}><b>{platformLabel(one.platform)}</b> <span>{plain(one.message || "Not available on this computer.", 160)}</span></p>
      ))}
      {notice ? <p className="dev-note" role="status">{notice}</p> : null}
      {devices.length === 0 ? <p className="dev-note">No devices are listed.</p> : (
        <ul className="dev-list" aria-label="Devices">
          {devices.map((one) => (
            <li key={one.id}>
              <button type="button" className="dev-row" aria-pressed={one.id === selected} disabled={busy} onClick={() => { setSelected(one.id); setFailure(null); setHingeOpen(false); }}>
                <Smartphone size={14} aria-hidden />
                <span className="dev-name">{plain(one.name, 60)}</span>
                <span className="dev-meta">{`${platformLabel(one.platform)} ${plain(one.version, 16)}`.trim()}</span>
                {one.foldable ? <span className="pill pill-accent">Foldable</span> : null}
                <span className="dev-state" data-state={one.state}>{stateLabel(one.state)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      {device ? (
        <div className="dev-view">
          <div className="dev-bar">
            <b>{plain(device.name, 60)}</b>
            <span className="dev-meta">{stateLabel(device.state)}</span>
            {fold ? <span className="dev-posture" aria-label="Posture">{readout(device)}</span> : null}
          </div>
          <div className="dev-stage">
            {shot?.id === device.id ? <img className="dev-shot" src={shot.src} alt={`Screenshot of ${plain(device.name, 40)}`} /> : <p className="dev-note">No screenshot yet. The live screen is Planned.</p>}
          </div>
          {device.state !== "booted" ? <p className="dev-note">Start this device in its own tools to use the controls.</p> : detail.status === "loading" ? <p className="dev-note">Reading what this device can do…</p> : detail.status === "error" ? <p className="field-error" role="alert">Could not read what this device can do.</p> : controls ? (
            <div className="dev-rail" role="toolbar" aria-label="Device controls">
              {controls.buttons ? <>{tool("Back", <ChevronLeft size={16} aria-hidden />, () => press("back"))}{tool("Home", <House size={16} aria-hidden />, () => press("home"))}{tool("Recents", <Square size={16} aria-hidden />, () => press("recents"))}</> : null}
              {controls.rotate ? tool("Rotate", <RotateCw size={16} aria-hidden />, rotate) : null}
              {controls.planned.map((name) => <IconButton key={name} label={`${name} (Planned)`} disabled>{name === "Rotate" ? <RotateCw size={16} aria-hidden /> : <House size={16} aria-hidden />}</IconButton>)}
              {controls.screenshot ? tool("Screenshot", <Camera size={16} aria-hidden />, () => run(device, { type: "screenshot" }, "take a screenshot of")) : null}
              {controls.powerOff ? tool("Power off", <Power size={16} aria-hidden />, () => run(device, { type: "power_off" }, "power off")) : null}
              {controls.fold ? <span className="dev-sep" aria-hidden /> : null}
              {controls.fold && fold?.posture !== "closed" ? tool("Fold", <FoldHorizontal size={16} aria-hidden />, () => run(device, { type: "fold" }, "fold")) : null}
              {controls.fold && (closed || fold?.posture == null) ? tool("Unfold", <UnfoldHorizontal size={16} aria-hidden />, () => run(device, { type: "unfold" }, "unfold")) : null}
              {controls.hinge ? tool("Hinge angle", <SlidersHorizontal size={16} aria-hidden />, () => setHingeOpen((open) => !open), { pressed: hingeOpen }) : null}
            </div>
          ) : null}
          {controls?.hinge && hingeOpen ? (
            <div className="dev-hinge" role="dialog" aria-label="Hinge angle">
              <div className="dev-hinge-head"><b>Hinge angle</b><span>{draft}°</span></div>
              <input type="range" aria-label="Angle in degrees" min={HINGE_MIN} max={HINGE_MAX} step={1} value={draft} disabled={busy} onChange={(event) => setDraft(clampAngle(Number(event.target.value)))} />
              <div className="dev-hinge-ends"><span>{HINGE_MIN}°</span><span>{HINGE_MAX}°</span></div>
              <div className="dev-presets">
                <Button small disabled={busy} onClick={() => run(device, { type: "hinge", angle: draft }, "set the hinge of")}>Set angle</Button>
                {PRESETS.map(({ preset, label }) => <Button key={preset} small tone="quiet" disabled={busy} onClick={() => run(device, { type: "hinge_preset", preset }, "set the hinge of")}>{label}</Button>)}
              </div>
              <p className="dev-note">Closed and Open are what Fold and Unfold do. The device answers with the posture it ends in.</p>
            </div>
          ) : null}
          {busy ? <p className="dev-note" role="status">Working on {plain(device.name, 40)}…</p> : null}
          {failure ? (
            <div className="dev-fail" role="alert"><span>{failure.text}</span><Button small tone="quiet" disabled={busy} onClick={failure.again}>Try again</Button></div>
          ) : null}
          <p className="dev-planned">{PLANNED.map((name) => <span className="pill" key={name}>{name} · Planned</span>)}</p>
        </div>
      ) : null}
    </section>
  );
}
