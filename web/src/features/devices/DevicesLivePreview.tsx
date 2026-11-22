import { useEffect, useState, type ReactNode } from "react";
import { Button, Pill } from "../../ui/controls";
import { setDevicesApi } from "./api";
import type { Device } from "./contract";
import { DeviceView } from "./DeviceView";
import { plain, platformLabel, screenshotSrc, stateLabel } from "./model";
import { setDevicesPlannedApi, type DevicePoint } from "./planned";
import "./devices.css";

/**
 * DEV-ONLY frames for the phase-2 device work (`#/preview/devices?frame=<id>`): the live screen sketch,
 * touch and typing, and the agent that drives a device. Production never loads this module: the route is
 * behind `__ORBYN_PREVIEWS__` and the build script checks that neither the frames nor the mock ship.
 */

export const LIVE_FRAMES = [
  "device-input-touch",
  "device-input-text",
  "device-input-android",
  "device-agent-driving",
  "device-agent-paused",
  "device-agent-tools",
] as const;

type ThreadKind = "driving" | "paused" | "tools";

interface LiveFrame {
  devices: Device[];
  device: Device;
  /** Omitted means the dev mock has no live stream, so the frame keeps the service's Planned note. */
  live: boolean;
  screen?: string;
  /** A screen for the thread's screenshot card, when the frame draws one. */
  shot?: string;
  cursor?: DevicePoint;
  agent?: "off" | "driving" | "paused";
  initialTyping?: boolean;
  initialText?: string;
  list?: boolean;
  thread?: ThreadKind;
}

interface Screens {
  IOS_KEYBOARD: string;
  IOS_LOGIN: string;
  IOS_TOUCH: string;
}

const iosDevice = (name: string, version: string): Device => ({
  id: `ios:${name.replaceAll(" ", "-")}`,
  name,
  platform: "ios",
  state: "booted",
  version,
  foldable: false,
});

const androidDevice = (): Device => ({
  id: "android:Pixel_8_API_35",
  name: "Pixel 8",
  platform: "android",
  state: "booted",
  version: "15",
  foldable: false,
  orientation: "portrait",
});

function configFor(frame: string, screens: Screens): LiveFrame {
  const iphone16 = iosDevice("iPhone 16", "18.2");
  if (frame === "device-input-touch") {
    const device = iosDevice("iPhone 17 Pro", "26.0");
    return { devices: [device], device, live: true, screen: screens.IOS_TOUCH };
  }
  if (frame === "device-input-text") {
    return { devices: [iphone16], device: iphone16, live: true, screen: screens.IOS_KEYBOARD, initialTyping: true, initialText: "user@example.com" };
  }
  if (frame === "device-input-android") {
    const device = androidDevice();
    return { devices: [device, iosDevice("iPhone 17 Pro", "26.0")], device, live: false, list: true };
  }
  if (frame === "device-agent-driving") {
    return { devices: [iphone16], device: iphone16, live: true, screen: screens.IOS_LOGIN, cursor: { x: 0.62, y: 0.36 }, agent: "driving", thread: "driving" };
  }
  if (frame === "device-agent-paused") {
    return { devices: [iphone16], device: iphone16, live: true, screen: screens.IOS_LOGIN, agent: "paused", thread: "paused" };
  }
  if (frame === "device-agent-tools") {
    return { devices: [iphone16], device: iphone16, live: true, screen: screens.IOS_LOGIN, shot: screens.IOS_LOGIN, thread: "tools" };
  }
  return { devices: [], device: iphone16, live: false };
}

function DeviceList({ devices }: { devices: Device[] }) {
  return (
    <ul className="dvp-list" aria-label="Devices">
      {devices.map((device) => (
        <li key={device.id}>
          <div className="dev-row">
            <span className="dev-name">{plain(device.name, 60)}</span>
            <span className="dev-meta">{`${platformLabel(device.platform)} ${plain(device.version, 16)}`.trim()}</span>
            <span className="dev-state" data-state={device.state}>{stateLabel(device.state)}</span>
          </div>
        </li>
      ))}
    </ul>
  );
}

function Tool({ name, result, action }: { name: string; result: string; action?: ReactNode }) {
  return (
    <div className="dlv-tool">
      <span className="dlv-tool-name">{name}</span>
      <span className="dlv-tool-result">{result}{action}</span>
    </div>
  );
}

function Thread({ kind, screenshot }: { kind: ThreadKind; screenshot?: string }) {
  const [viewer, setViewer] = useState(false);
  const shot = screenshot ? screenshotSrc(screenshot) : null;
  if (kind === "tools") {
    return (
      <section className="dlv-thread" aria-label="Thread">
        <div className="dlv-thread-head"><b>Check how the sign-in screen looks on the iPhone simulator.</b><span className="dlv-thread-status">Running</span></div>
        <div className="dlv-tools">
          <Tool name="device_list" result="2 devices · iPhone 16 running, Pixel 8 shut down" />
          <Tool name="device_open" result="iPhone 16 · iOS 18.2 · 3.1 s" />
          <Tool name="device_screenshot" result="1170 × 2532 · added to the thread" action={<Button small tone="quiet" onClick={() => setViewer(true)}>Open</Button>} />
          <Tool name="device_close" result="Released the device. It keeps running." />
        </div>
        <figure className="dlv-shot">
          {shot ? <img src={shot} alt="Screenshot of iPhone 16" /> : null}
          <figcaption>iPhone 16 · 14:31:55 · 1170 × 2532</figcaption>
        </figure>
        <p className="dlv-msg" data-kind="agent">The Sign in button sits below the fold on this screen size.</p>
        {viewer && shot ? (
          <div className="dlv-viewer" role="dialog" aria-label="Screenshot">
            <div className="dlv-viewer-card">
              <img src={shot} alt="Screenshot of iPhone 16 at full size" />
              <div className="dlv-viewer-actions"><Button small tone="quiet" onClick={() => setViewer(false)}>Close</Button></div>
            </div>
          </div>
        ) : null}
      </section>
    );
  }
  const paused = kind === "paused";
  return (
    <section className="dlv-thread" aria-label="Thread">
      <div className="dlv-thread-head">
        <b>{paused ? "Waiting for you" : "Running"}</b>
        <span className="dlv-thread-status" data-waiting={paused || undefined}>Opus 5.5 High</span>
      </div>
      <p className="dlv-msg" data-kind="user">Open the sign-in screen on the iPhone simulator and check that the keyboard does not cover the button.</p>
      {paused ? (
        <div className="dlv-tools">
          <p className="dlv-msg">Opened iPhone 16 · iOS 18.2</p>
          <p className="dlv-msg">Took a screenshot of the device</p>
          <p className="dlv-msg">Tapped Email</p>
          <p className="dlv-msg" data-kind="agent">Waiting for you to hand the device back</p>
        </div>
      ) : (
        <div className="dlv-tools">
          <p className="dlv-msg">Opened iPhone 16 · iOS 18.2</p>
          <p className="dlv-msg">Tapping Email</p>
        </div>
      )}
      <p className="dlv-msg" data-kind="agent">The keyboard covers the Sign in button on this screen size. I will scroll the form up when the keyboard opens.</p>
    </section>
  );
}

export function DevicesLivePreview({ frame }: { frame: string }) {
  const [config, setConfig] = useState<LiveFrame | null>(null);

  useEffect(() => {
    let live = true;
    void Promise.all([import("../../../mock/devices"), import("../../../mock/deviceScreens")]).then(([m, screens]) => {
      if (!live) return;
      const next = configFor(frame, screens);
      setDevicesApi(m.createDevicesMock({ devices: next.devices, platforms: [{ platform: "android", available: true }, { platform: "ios", available: true }], delayMs: 0 }));
      setDevicesPlannedApi(m.createDevicesPlannedMock({
        supported: true,
        live: next.live ? { screen: next.screen, cursor: next.cursor, agent: next.agent ?? "off", agentName: "Claude Code" } : false,
      }));
      setConfig(next);
    });
    return () => {
      live = false;
      setDevicesApi(null);
      setDevicesPlannedApi(null);
    };
  }, [frame]);

  if (!config) return null;
  return (
    <div className={config.thread ? "dvp-stage dvp-live" : "dvp-stage"}>
      {config.thread ? <Thread kind={config.thread} screenshot={config.shot} /> : null}
      <div className="dvp-panel">
        <div className="dev">
          <div className="dev-head"><div className="dev-head-title"><b>Devices</b><Pill>Planned</Pill></div></div>
          {config.list ? <DeviceList devices={config.devices} /> : null}
          <DeviceView
            key={config.device.id}
            device={config.device}
            onChanged={() => {}}
            onGone={() => {}}
            onRestart={() => {}}
            initialTyping={config.initialTyping}
            initialText={config.initialText}
          />
        </div>
      </div>
    </div>
  );
}
