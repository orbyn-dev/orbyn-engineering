import { Globe, Laptop, MoreHorizontal, Smartphone } from "lucide-react";
import { useState } from "react";
import type { AccountDevice } from "../../generated/AccountDevice";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { appLabel, deviceLine } from "./deviceCopy";

const iconFor = (device: AccountDevice) => {
  if (/ios|android/i.test(device.platform)) return Smartphone;
  if (/web/i.test(device.platform)) return Globe;
  return Laptop;
};

/** The device rows, the same on the Account page and on the Devices page. Each other device has a menu; this one has none. */
export function DeviceRows({ devices, syncedAt, onRemove }: { devices: AccountDevice[]; syncedAt: number | null; onRemove?(device: AccountDevice): void }) {
  const [menu, setMenu] = useState<{ device: AccountDevice; x: number; y: number } | null>(null);
  return (
    <>
      <ul className="ac-devs">
        {devices.map((device) => {
          const Icon = iconFor(device);
          return (
            <li key={device.id} className="ac-row ac-dev" aria-label={device.name}>
              <span className="ac-di" aria-hidden><Icon size={16} /></span>
              <div>
                <b>{device.name}</b>
                <small>{device.platform}</small>
                <small>{deviceLine(device, syncedAt)}</small>
              </div>
              <span className="ac-tags">
                {device.current ? <span className="ac-tag">This device</span> : null}
                <span className="ac-tag">{appLabel(device)}</span>
              </span>
              {onRemove && !device.current ? (
                <button
                  type="button"
                  className="icon-btn"
                  aria-label={`Device menu for ${device.name}`}
                  title="Device menu"
                  aria-haspopup="menu"
                  onClick={(event) => {
                    const rect = event.currentTarget.getBoundingClientRect();
                    setMenu({ device, x: rect.right - 160, y: rect.bottom + 4 });
                  }}
                >
                  <MoreHorizontal size={16} aria-hidden />
                </button>
              ) : (
                <span className="ac-nomenu" aria-hidden />
              )}
            </li>
          );
        })}
      </ul>
      <Presence>
        {menu && onRemove ? (
          <Menu
            x={menu.x}
            y={menu.y}
            origin="top left"
            label={`${menu.device.name} menu`}
            items={[{ label: "Remove", destructive: true, onSelect: () => onRemove(menu.device) }]}
            onClose={() => setMenu(null)}
          />
        ) : null}
      </Presence>
    </>
  );
}

/** Every trusted device can open all of the account's Orbyn data until per-app access exists; the copy never implies otherwise. */
export function DevicesNote({ platform }: { platform: string }) {
  return (
    <>
      <div className="ac-note">
        <div>
          <b>Every trusted device can open all of your Orbyn data</b>, including the data of other Orbyn apps. Orbyn can’t limit a device to one app yet. Remove a device you no longer trust.
        </div>
      </div>
      {platform === "windows" ? (
        <div className="ac-note">
          <div>
            <b>On Windows, any program you run can act as this device.</b> Keep this account protected.
          </div>
        </div>
      ) : null}
    </>
  );
}
