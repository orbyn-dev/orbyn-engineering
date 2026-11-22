import { MoreHorizontal, Plus, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Pill } from "../../ui/controls";
import { Menu, type MenuItem } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import type { Device } from "./contract";
import { plain, tabName } from "./model";

interface Editing { id: string; value: string }

/**
 * The per-device tabs inside the Devices tab. A tab shows the person's label (the device name until renamed), closes
 * without shutting the device down, and its menu holds Float over chat, Rename, Setup and tools, Agent access and Shut
 * down. "+" lists the devices the service gave; there is no create. Renaming is inline: Return saves, Escape cancels,
 * and an empty name goes back to the device name.
 */
export function DeviceTabs({ devices, open, active, names, busy, onSelect, onClose, onRename, onFloat, onSetup, onShutdown }: {
  devices: Device[];
  open: string[];
  active: string | null;
  names: Record<string, string>;
  busy: boolean;
  onSelect(id: string): void;
  onClose(id: string): void;
  onRename(id: string, name: string): void;
  onFloat(id: string): void;
  onSetup(): void;
  onShutdown(device: Device): void;
}) {
  const [picker, setPicker] = useState<{ x: number; y: number } | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; id: string } | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const cancelled = useRef(false);

  const editingId = editing?.id;
  useEffect(() => {
    if (editingId) input.current?.select();
  }, [editingId]);

  const deviceOf = (id: string) => devices.find((device) => device.id === id);
  const labelOf = (device: Device) => tabName(names[device.id], plain(device.name, 60));
  const tabs = open.map(deviceOf).filter((device): device is Device => device !== undefined);
  const pickable = devices.filter((device) => !open.includes(device.id));
  const chosen = menu ? deviceOf(menu.id) : undefined;

  const save = () => {
    if (!editing) return;
    onRename(editing.id, editing.value);
    setEditing(null);
  };
  const pickerItems: MenuItem[] = pickable.length === 0
    ? [{ label: "Every listed device is open", disabled: true, content: <span className="dev-menu-note">Every listed device is open</span> }]
    : pickable.map((device) => ({
        label: `${labelOf(device)}${device.state === "booted" ? ", Running" : `, ${device.state === "shutdown" ? "Shut down" : "Unknown"}`}`,
        icon: <Plus size={14} aria-hidden />,
        onSelect: () => onSelect(device.id),
      }));
  const menuItems: MenuItem[] = chosen ? [
    { label: "Float over chat", onSelect: () => onFloat(chosen.id) },
    { label: "Rename", onSelect: () => { cancelled.current = false; setEditing({ id: chosen.id, value: labelOf(chosen) }); } },
    { label: "Device tools", disabled: true, title: "The device tools drawer is Planned.", content: <span className="dev-menu-note">Device tools <Pill>Planned</Pill></span> },
    { label: "Setup and tools…", separatorBefore: true, onSelect: onSetup },
    { label: "Agent access…", disabled: true, title: "The agent-access toggle is Planned.", content: <span className="dev-menu-note">Agent access… <Pill>Planned</Pill></span> },
    ...(chosen.state === "booted" ? [{ label: "Shut down", destructive: true, onSelect: () => onShutdown(chosen) }] : []),
  ] : [];

  return (
    <div className="dev-tabs">
      <div className="dev-tab-row" role="tablist" aria-label="Device tabs">
        {tabs.map((device) => {
          const name = labelOf(device);
          const on = device.id === active;
          return (
            <div className="dev-tab-slot" key={device.id} data-on={on || undefined}>
              {editing?.id === device.id ? (
                <input
                  ref={input}
                  className="dev-tab-edit"
                  aria-label={`Rename ${name}`}
                  value={editing.value}
                  maxLength={60}
                  disabled={busy}
                  onChange={(event) => setEditing({ id: device.id, value: event.target.value })}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") { event.preventDefault(); save(); }
                    else if (event.key === "Escape") { event.preventDefault(); cancelled.current = true; setEditing(null); }
                  }}
                  onBlur={() => { if (cancelled.current) { cancelled.current = false; return; } save(); }}
                />
              ) : (
                <button
                  type="button"
                  role="tab"
                  className="dev-tab"
                  aria-selected={on}
                  title={name}
                  onClick={() => onSelect(device.id)}
                  onDoubleClick={() => { cancelled.current = false; setEditing({ id: device.id, value: name }); }}
                >
                  {name}
                </button>
              )}
              {on ? (
                <button
                  type="button"
                  className="dev-tab-more"
                  aria-label="Device tab menu"
                  aria-haspopup="menu"
                  aria-expanded={menu !== null}
                  onClick={(event) => {
                    const rect = event.currentTarget.getBoundingClientRect();
                    setMenu(menu ? null : { x: rect.right - 200, y: rect.bottom + 4, id: device.id });
                  }}
                >
                  <MoreHorizontal size={14} aria-hidden />
                </button>
              ) : null}
              <button type="button" className="dev-tab-close" aria-label={`Close ${name} tab`} disabled={busy} onClick={() => onClose(device.id)}>
                <X size={12} aria-hidden />
              </button>
            </div>
          );
        })}
        <button
          type="button"
          className="dev-tab-add"
          aria-label="Open another device"
          aria-haspopup="menu"
          aria-expanded={picker !== null}
          onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            setPicker(picker ? null : { x: rect.right - 220, y: rect.bottom + 4 });
          }}
        >
          <Plus size={14} aria-hidden />
        </button>
      </div>
      {picker ? <Presence><Menu x={picker.x} y={picker.y} label="Open another device" items={pickerItems} onClose={() => setPicker(null)} /></Presence> : null}
      {menu ? <Presence><Menu x={menu.x} y={menu.y} label="Device tab menu" items={menuItems} onClose={() => setMenu(null)} /></Presence> : null}
    </div>
  );
}
