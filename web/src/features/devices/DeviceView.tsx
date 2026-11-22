import { Camera, ChevronLeft, FoldHorizontal, House, Power, RotateCw, SlidersHorizontal, Square, UnfoldHorizontal } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { ApiFailure } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import { Button, IconButton, Pill } from "../../ui/controls";
import { devicesApi, useApiVersion } from "./api";
import { HINGE_MAX, HINGE_MIN, type ButtonName, type Device, type DeviceAction, type HingePreset } from "./contract";
import { clampAngle, controlsFor, hingeLabel, nextOrientation, orientationLabel, plain, postureLabel, screenshotSrc, stateLabel } from "./model";
import { devicesPlannedApi, restartLabel } from "./planned";
import { DeviceToolsButton } from "./tools/DeviceTools";

const PRESETS: readonly { preset: HingePreset; label: string }[] = [{ preset: "closed", label: "Closed" }, { preset: "half_open", label: "Half open" }, { preset: "open", label: "Open" }];
/** What the frames draw that no code can do yet: the live view (H22/H23), Record and typing. The tools drawer (H26) opens but is Planned without its service route. */
const PLANNED = ["Live screen", "Record", "Type text"];

interface Failure { text: string; again(): void }

const readout = (device: Device): string | null => {
  if (!device.fold) return null;
  const { posture, hinge_angle } = device.fold;
  return typeof hinge_angle === "number" ? `${postureLabel(posture)} · ${hingeLabel(hinge_angle)}` : postureLabel(posture);
};

/**
 * One device's bar, stage and toolbar. It reads its own `GET /api/devices/{id}` and draws only the actions the
 * service lists. A `device_not_found` on that read or on a write tells the panel the device is gone. The live
 * screen, typing and the tools drawer are Planned pills; restarting a frozen or crashed device goes through the
 * Planned client, which answers nothing in production.
 */
export function DeviceView({ device, onChanged, onGone, onRestart }: {
  device: Device;
  onChanged(device: Device): void;
  onGone(id: string, message?: string): void;
  onRestart(): void;
}) {
  const seen = useApiVersion();
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [shot, setShot] = useState<{ id: string; src: string } | null>(null);
  const [hingeOpen, setHingeOpen] = useState(false);
  const [draft, setDraft] = useState(90);
  const gone = useRef(false);
  const answered = useRef(false);
  const detail = useAsync(`device:${seen}:${device.id}`, (signal) => devicesApi().detail(device.id, signal));
  const angle = device.fold?.hinge_angle ?? null;
  useEffect(() => { setDraft(clampAngle(angle ?? 90)); }, [angle, device.id]);
  useEffect(() => { setFailure(null); setHingeOpen(false); gone.current = false; answered.current = false; }, [device.id]);

  const detailGone = detail.status === "error" && detail.error.code === "device_not_found";
  useEffect(() => {
    if (!detailGone || gone.current || detail.status !== "error") return;
    gone.current = true;
    onGone(device.id, detail.error.message);
  }, [detailGone, detail, device.id, onGone]);

  const drop = (id: string, message?: string) => {
    if (gone.current) return;
    gone.current = true;
    onGone(id, message);
  };

  const run = async (body: DeviceAction, verb: string): Promise<void> => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setFailure(null);
    const fail = (text: string) => setFailure({ text, again: () => { void run(body, verb); } });
    try {
      const result = await devicesApi().act(device.id, body);
      if (typeof result.device === "object" && result.device !== null && result.device.id === device.id) {
        answered.current = true;
        const next = { ...device, ...result.device };
        // The service read the state back: an orientation it did not report is not carried over.
        if (result.device.orientation === undefined) delete next.orientation;
        onChanged(next);
      }
      if (body.type === "screenshot") {
        const src = screenshotSrc(result.image);
        if (src) setShot({ id: device.id, src });
        else fail("The service sent something that is not a usable screenshot.");
      }
      if (body.type === "hinge" || body.type === "hinge_preset" || body.type === "fold" || body.type === "unfold") setHingeOpen(false);
    } catch (error) {
      if (error instanceof ApiFailure && error.code === "device_not_found") {
        drop(device.id, error.message);
        return;
      }
      const message = error instanceof ApiFailure ? plain(error.message, 200) : "Something went wrong.";
      fail(`Couldn’t ${verb} ${plain(device.name, 40)}. ${message}`);
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const restart = async () => {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setFailure(null);
    try {
      const answer = await devicesPlannedApi().restart(device.id);
      if (answer === null) setFailure({ text: "Restarting a device is Planned. The service has no such action yet.", again: () => { void restart(); } });
      else onRestart();
    } catch (error) {
      const message = error instanceof ApiFailure ? plain(error.message, 200) : "Something went wrong.";
      setFailure({ text: `Couldn’t restart ${plain(device.name, 40)}. ${message}`, again: () => { void restart(); } });
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const actions = detail.status === "ready" && detail.data && detail.data.id === device.id ? detail.data.actions : [];
  const controls = controlsFor(device, Array.isArray(actions) ? actions : []);
  const offered = controls.buttons || controls.rotate || controls.screenshot || controls.powerOff || controls.fold || controls.hinge || controls.planned.length > 0;
  const fold = device.fold;
  const closed = fold?.posture === "closed";
  const detailOrientation = detail.status === "ready" && detail.data.id === device.id ? detail.data.orientation : undefined;
  const currentOrientation = answered.current ? device.orientation : detailOrientation ?? device.orientation;
  const orientation = orientationLabel(currentOrientation);
  const press = (name: ButtonName) => void run({ type: "button", value: name }, `press ${name} on`);
  const rotate = () => void run({ type: "rotate", value: nextOrientation(currentOrientation) }, "rotate");
  const tool = (label: string, icon: ReactNode, onClick: () => void, extra: { pressed?: boolean } = {}) => (
    <IconButton key={label} label={label} onClick={onClick} disabled={busy} pressed={extra.pressed}>{icon}</IconButton>
  );
  const overlay = device.state === "frozen" || device.state === "crashed" ? (
    <div className="dev-overlay" data-tone={device.state === "crashed" ? "danger" : undefined} role="alert">
      <b>{device.state === "frozen" ? "Not responding" : "The simulator stopped"}</b>
      <p>{device.state === "frozen" ? "The simulator has not answered." : "The device process exited."}</p>
      <Button small tone={device.state === "crashed" ? "danger" : "secondary"} disabled={busy} onClick={() => void restart()}>{restartLabel(device.state)}</Button>
    </div>
  ) : null;

  return (
    <div className="dev-view">
      <div className="dev-bar">
        <b title={plain(device.name, 60)}>{plain(device.name, 60)}</b>
        <span className="dev-meta">{stateLabel(device.state)}</span>
        {orientation ? <span className="dev-orientation" aria-label="Orientation">{orientation}</span> : null}
        {fold ? <span className="dev-posture" aria-label="Posture">{readout(device)}</span> : null}
      </div>
      <div className="dev-stage" data-state={device.state}>
        {shot?.id === device.id ? <img className="dev-shot" src={shot.src} alt={`Screenshot of ${plain(device.name, 40)}`} /> : device.state === "frozen" || device.state === "crashed" ? null : <p className="dev-note">No screenshot yet. The live screen is Planned.</p>}
        {overlay}
      </div>
      {device.state === "shutdown" ? <p className="dev-note">Start this device in its own tools to use the controls.</p> : device.state === "frozen" || device.state === "crashed" ? null : detail.status === "loading" ? <p className="dev-note" role="status">Reading what this device can do…</p> : detail.status === "error" ? detailGone ? null : <p className="field-error" role="alert">Could not read what this device can do.</p> : controls && offered ? (
        <div className="dev-rail" role="toolbar" aria-label="Device controls">
          {controls.buttons ? <>{tool("Back", <ChevronLeft size={16} aria-hidden />, () => press("back"))}{tool("Home", <House size={16} aria-hidden />, () => press("home"))}{tool("Recents", <Square size={16} aria-hidden />, () => press("recents"))}</> : null}
          {controls.rotate ? tool("Rotate", <RotateCw size={16} aria-hidden />, rotate) : null}
          {controls.planned.map((name) => <IconButton key={name} label={`${name} (Planned)`} disabled>{name === "Rotate" ? <RotateCw size={16} aria-hidden /> : <House size={16} aria-hidden />}</IconButton>)}
          {controls.screenshot ? tool("Screenshot", <Camera size={16} aria-hidden />, () => void run({ type: "screenshot" }, "take a screenshot of")) : null}
          {controls.powerOff ? tool("Power off", <Power size={16} aria-hidden />, () => void run({ type: "power_off" }, "power off")) : null}
          {controls.fold ? <span className="dev-sep" aria-hidden /> : null}
          {controls.fold && fold?.posture !== "closed" ? tool("Fold", <FoldHorizontal size={16} aria-hidden />, () => void run({ type: "fold" }, "fold")) : null}
          {controls.fold && (closed || fold?.posture == null) ? tool("Unfold", <UnfoldHorizontal size={16} aria-hidden />, () => void run({ type: "unfold" }, "unfold")) : null}
          {controls.hinge ? tool("Hinge angle", <SlidersHorizontal size={16} aria-hidden />, () => setHingeOpen((open) => !open), { pressed: hingeOpen }) : null}
        </div>
      ) : null}
      {controls?.hinge && hingeOpen ? (
        <div className="dev-hinge" role="dialog" aria-label="Hinge angle">
          <div className="dev-hinge-head"><b>Hinge angle</b><span>{draft}°</span></div>
          <input type="range" aria-label="Angle in degrees" min={HINGE_MIN} max={HINGE_MAX} step={1} value={draft} disabled={busy} onChange={(event) => setDraft(clampAngle(Number(event.target.value)))} />
          <div className="dev-hinge-ends"><span>{HINGE_MIN}°</span><span>{HINGE_MAX}°</span></div>
          <div className="dev-presets">
            <Button small disabled={busy} onClick={() => void run({ type: "hinge", angle: draft }, "set the hinge of")}>Set angle</Button>
            {PRESETS.map(({ preset, label }) => <Button key={preset} small tone="quiet" disabled={busy} onClick={() => void run({ type: "hinge_preset", preset }, "set the hinge of")}>{label}</Button>)}
          </div>
          <p className="dev-note">Closed and Open are what Fold and Unfold do. The device answers with the posture it ends in.</p>
        </div>
      ) : null}
      {busy ? <p className="dev-note" role="status">Working on {plain(device.name, 40)}…</p> : null}
      {failure ? (
        <div className="dev-fail" role="alert"><span>{failure.text}</span><Button small tone="quiet" disabled={busy} onClick={failure.again}>Try again</Button></div>
      ) : null}
      <p className="dev-planned">{PLANNED.map((name) => <Pill key={name}>{name} · Planned</Pill>)}<DeviceToolsButton device={device} /></p>
    </div>
  );
}
