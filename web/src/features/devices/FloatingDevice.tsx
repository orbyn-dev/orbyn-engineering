import { IconButton } from "../../ui/controls";
import { X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useUi } from "../../product/state/ui";
import { useDevices } from "./api";
import { plain, stateLabel, tabName } from "./model";

const MIN_WIDTH = 280;
const MAX_WIDTH = 720;
/**
 * The one device floating over the chat (H22). It is a person's own window: its place and width are kept in the
 * preference store and written with try/catch browser storage. Drag the title to move, drag the corner to resize,
 * Dock puts it back in the Devices panel and closing it leaves the device alone. The stage is the built screenshot
 * stage: the live screen and typed input are Planned (phase 2).
 */
export function DevicesFloat({ threadId }: { threadId: string | null }) {
  const float = useUi((state) => state.devices.float);
  if (!threadId || !float) return null;
  return <FloatWindow key={threadId} threadId={threadId} />;
}

function FloatWindow({ threadId }: { threadId: string }) {
  const float = useUi((state) => state.devices.float);
  const names = useUi((state) => state.devices.names);
  const devices = useUi((state) => state.devices);
  const setPrefs = useUi((state) => state.setPrefs);
  const { list, settled } = useDevices();
  const device = list?.devices.find((one) => one.id === float?.id) ?? null;
  const [drag, setDrag] = useState<{ x: number; y: number } | null>(null);
  const [width, setWidth] = useState(float?.width ?? 360);
  const move = useRef<{ pointer: number; x: number; y: number; left: number; top: number } | null>(null);
  const size = useRef<{ pointer: number; x: number; start: number } | null>(null);

  useEffect(() => {
    if (float) setWidth(float.width);
  }, [float]);

  useEffect(() => {
    if (!float) return;
    if ((settled && list === null) || (list !== null && !list.devices.some((one) => one.id === float.id))) {
      setPrefs({ devices: { ...devices, float: null } });
    }
  }, [list, settled, float, devices, setPrefs]);

  if (!float) return null;
  const label = device ? tabName(names[device.id], plain(device.name, 60)) : "Device";
  const left = drag?.x ?? float.x;
  const top = drag?.y ?? float.y;
  const commit = (next: { x: number; y: number; width: number }) => {
    const maxX = Math.max(0, window.innerWidth - 120);
    const maxY = Math.max(0, window.innerHeight - 80);
    setPrefs({
      devices: {
        ...devices,
        float: {
          id: float.id,
          x: Math.min(maxX, Math.max(0, Math.round(next.x))),
          y: Math.min(maxY, Math.max(0, Math.round(next.y))),
          width: Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, Math.round(next.width))),
        },
      },
    });
  };

  return (
    <aside className="dev-float" aria-label="Floating device" style={{ left, top, width }}>
      <div
        className="dev-float-bar"
        onPointerDown={(event) => {
          if (event.button !== 0 || (event.target as Element).closest("button")) return;
          event.currentTarget.setPointerCapture?.(event.pointerId);
          move.current = { pointer: event.pointerId, x: event.clientX, y: event.clientY, left, top };
        }}
        onPointerMove={(event) => {
          const held = move.current;
          if (!held || held.pointer !== event.pointerId) return;
          setDrag({ x: held.left + event.clientX - held.x, y: held.top + event.clientY - held.y });
        }}
        onPointerUp={(event) => {
          const held = move.current;
          if (!held || held.pointer !== event.pointerId) return;
          move.current = null;
          commit({ x: held.left + event.clientX - held.x, y: held.top + event.clientY - held.y, width });
          setDrag(null);
        }}
        onPointerCancel={() => { move.current = null; setDrag(null); }}
      >
        <b title={label}>{label}</b>
        <span className="dev-meta">{device ? stateLabel(device.state) : "Reading…"}</span>
        <span className="dev-float-end">
          <button
            type="button"
            className="dev-float-dock"
            onClick={() => {
              setPrefs({ devices: { ...devices, active: float.id, float: null } });
              useUi.getState().reopenPanelTab("devices");
            }}
          >
            Dock
          </button>
          <IconButton label="Close floating device" onClick={() => setPrefs({ devices: { ...devices, float: null } })}>
            <X size={14} aria-hidden />
          </IconButton>
        </span>
      </div>
      <div className="dev-float-stage">
        <p className="dev-note">{device?.state === "booted" ? "No screenshot yet. The live screen is Planned." : device ? `${stateLabel(device.state)}. Start this device in its own tools to use the controls.` : "Reading the device…"}</p>
      </div>
      <span
        className="dev-float-resize"
        role="separator"
        aria-label="Resize floating device"
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          event.currentTarget.setPointerCapture?.(event.pointerId);
          size.current = { pointer: event.pointerId, x: event.clientX, start: width };
        }}
        onPointerMove={(event) => {
          const held = size.current;
          if (!held || held.pointer !== event.pointerId) return;
          setWidth(Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, held.start + event.clientX - held.x)));
        }}
        onPointerUp={(event) => {
          const held = size.current;
          if (!held || held.pointer !== event.pointerId) return;
          size.current = null;
          commit({ x: left, y: top, width: held.start + event.clientX - held.x });
        }}
        onPointerCancel={() => { size.current = null; setWidth(float.width); }}
      />
      <span className="sr-only">Floating {label} for thread {threadId}</span>
    </aside>
  );
}
