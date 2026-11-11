import { Button } from "../../ui/controls";
import type { AccountDevice } from "../../generated/AccountDevice";
import { appLabel, deviceLine } from "./deviceCopy";

/** The device rows, the same on the Account page and on the Devices page. Remove is offered on every device but this one. */
export function DeviceRows({ devices, syncedAt, onRemove }: { devices: AccountDevice[]; syncedAt: number | null; onRemove?(device: AccountDevice): void }) {
  return (
    <ul className="ac-devs">
      {devices.map((device) => (
        <li key={device.id} className="ac-row" aria-label={device.name}>
          <div>
            <b>{device.name}</b>
            <small>{device.platform}</small>
            <small>{deviceLine(device, syncedAt)}</small>
            <span className="ac-tags">
              {device.current ? <span className="ac-tag">This device</span> : null}
              <span className="ac-tag">{appLabel(device)}</span>
            </span>
          </div>
          {onRemove && !device.current ? (
            <Button small onClick={() => onRemove(device)} aria-label={`Remove ${device.name}`}>Remove</Button>
          ) : null}
        </li>
      ))}
    </ul>
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
