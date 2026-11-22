import { useEffect, useState, useSyncExternalStore } from "react";
import { Pill } from "../../ui/controls";
import { useUi } from "../../product/state/ui";
import type { Device, PlatformStatus } from "./contract";
import { setDevicesApi } from "./api";
import { DevicesFloat } from "./FloatingDevice";
import { DevicesPanel } from "./DevicesPanel";
import { SetupHub } from "./setup/SetupHub";
import { ToolVersions } from "./setup/ToolVersions";
import { setDevicesPlannedApi } from "./planned";
import "./devices.css";

/**
 * The dev-only page for the device-* frames (`#/preview/devices?frame=<id>`). It installs the mock clients and renders the
 * real panel, setup hub or tool-version page, so a frame can be checked without a service. It ships nowhere: the route and
 * this module live behind `__ORBYN_PREVIEWS__`, which the production build turns off.
 */

const SETUP = new Set([
  "device-setup-hub",
  "device-setup-platform-windows",
  "device-setup-tools-ios",
  "device-setup-tools-ios-missing",
  "device-setup-tools-android",
  "device-setup-installing",
  "device-setup-failed",
  "device-setup-access",
  "device-setup-done",
]);
const VERSIONS = new Set(["device-tool-versions", "device-tool-versions-checking", "device-tool-versions-offline", "device-tool-versions-steps"]);
const STATES = new Set(["device-running", "device-frozen", "device-crashed", "device-shutdown"]);
const FRAMES = [
  ...SETUP,
  ...VERSIONS,
  ...STATES,
  "device-none",
  "device-panel-list",
  "device-panel-list-narrow",
  "device-platform-unavailable",
  "device-platform-unavailable-ios",
  "device-platform-unavailable-both",
  "device-reading-actions",
  "device-reading-failed",
  "device-reading-actions-failed",
  "device-picker",
  "device-tabs",
  "device-tab-rename",
  "device-float",
  "device-float-docked-hint",
  "device-narrow-panel",
  "device-narrow-window",
];

const frameOf = (hash: string): string => {
  const frame = new URLSearchParams(hash.split("?")[1] ?? "").get("frame") ?? "";
  return FRAMES.includes(frame) ? frame : "device-panel-list";
};

const IOS_MISSING = "Xcode was not found. Install Xcode, then try again.";
const ANDROID_MISSING = "Android tools were not found. Install Android platform-tools and the emulator.";

function installFor(frame: string) {
  if (frame === "device-setup-installing") {
    return { state: "installing" as const, done: 212, total: 340, line: "Downloading agent-device-0.9.3.tgz", lines: ["$ orbyn install agent-device", "[=====     ] 62% Downloading agent-device-0.9.3.tgz"] };
  }
  if (frame === "device-setup-failed") {
    return { state: "failed" as const, done: 212, total: 340, line: "Could not download. The connection dropped at 212 KB.", error: "Connection reset (ECONNRESET)", lines: ["$ orbyn install agent-device", "Error: Connection reset (ECONNRESET)"] };
  }
  return null;
}

interface Prepared {
  narrow: boolean;
  platforms: PlatformStatus[];
  deviceToOpen: Device | null;
}

function Frame({ frame }: { frame: string }) {
  const [prepared, setPrepared] = useState<Prepared | null>(null);

  useEffect(() => {
    let live = true;
    void import("../../../mock/devices").then((m) => {
      if (!live) return;
      const planned = m.createDevicesPlannedMock({
        iosMissing: frame === "device-setup-tools-ios-missing",
        offline: frame === "device-tool-versions-offline",
        delayMs: frame === "device-tool-versions-checking" ? 3000 : 0,
        // The platform-unavailable frames show the list and its sentences, not the hub.
        supported: !frame.startsWith("device-platform-unavailable") && frame !== "device-reading-failed",
      });
      let devices = m.deviceFixtures();
      let platforms: PlatformStatus[] = [{ platform: "android", available: true }, { platform: "ios", available: true }];
      let open: string[] = [];
      let active: string | null = null;
      if (frame === "device-none") devices = [];
      if (frame === "device-platform-unavailable") {
        platforms = [
          { platform: "android", available: false, code: "tool_missing", message: ANDROID_MISSING },
          { platform: "ios", available: true },
        ];
        devices = devices.filter((one) => one.platform === "ios");
      }
      if (frame === "device-platform-unavailable-ios") {
        platforms = [
          { platform: "android", available: true },
          { platform: "ios", available: false, code: "tool_missing", message: IOS_MISSING },
        ];
        devices = devices.filter((one) => one.platform === "android");
      }
      if (frame === "device-platform-unavailable-both") {
        platforms = [
          { platform: "android", available: false, code: "tool_missing", message: ANDROID_MISSING },
          { platform: "ios", available: false, code: "tool_missing", message: IOS_MISSING },
        ];
        devices = [];
      }
      if (frame === "device-reading-failed") {
        platforms = [
          { platform: "android", available: false, code: "tool_failed", message: "Couldn’t read the devices. Try again." },
          { platform: "ios", available: true },
        ];
        devices = devices.filter((one) => one.platform === "ios");
      }
      if (STATES.has(frame)) {
        const state = frame.replace("device-", "") as "running" | "frozen" | "crashed" | "shutdown";
        devices = [m.stateFixture(state === "running" ? "booted" : state)];
        open = [devices[0]!.id];
        active = devices[0]!.id;
      }
      if (frame === "device-reading-actions" || frame === "device-reading-actions-failed") {
        devices = [m.deviceFixtures()[1]!, m.deviceFixtures()[3]!];
        open = [devices[0]!.id];
        active = devices[0]!.id;
      }
      if (frame === "device-tabs" || frame === "device-tab-rename" || frame === "device-float-docked-hint") {
        open = [m.deviceFixtures()[0]!.id, m.deviceFixtures()[3]!.id];
        active = m.deviceFixtures()[0]!.id;
      }
      if (frame === "device-narrow-panel" || frame === "device-narrow-window" || frame === "device-panel-list-narrow") {
        open = frame === "device-panel-list-narrow" ? [] : [m.deviceFixtures()[3]!.id];
        active = open[0] ?? null;
      }
      if (frame === "device-setup-tools-ios-missing") {
        platforms = [{ platform: "ios", available: false, code: "tool_missing", message: IOS_MISSING }];
      }
      if (frame === "device-setup-tools-android") {
        platforms = [{ platform: "android", available: false, code: "tool_missing", message: ANDROID_MISSING }];
      }
      const mock = m.createDevicesMock({ devices, platforms, delayMs: 0 });
      if (frame === "device-reading-actions") mock.detail = () => new Promise(() => {});
      if (frame === "device-reading-actions-failed") mock.failNextDetail("tool_failed");
      setDevicesApi(mock);
      setDevicesPlannedApi(planned);
      useUi.setState({
        devices: {
          open,
          active,
          names: frame === "device-tab-rename" ? { [m.deviceFixtures()[0]!.id]: "Checkout flow" } : {},
          float: frame === "device-float" ? { id: m.deviceFixtures()[3]!.id, x: 48, y: 96, width: 360 } : null,
        },
      });
      setPrepared({ narrow: frame.includes("narrow") || frame === "device-narrow-window", platforms, deviceToOpen: m.deviceFixtures()[3]! });
    });
    return () => {
      live = false;
      setDevicesApi(null);
      setDevicesPlannedApi(null);
    };
  }, [frame]);

  if (!prepared) return null;
  const initialStep = frame === "device-setup-access" ? 3 : frame === "device-setup-done" ? "done" : ["device-setup-tools-ios", "device-setup-tools-ios-missing", "device-setup-tools-android", "device-setup-installing", "device-setup-failed"].includes(frame) ? 2 : 1;
  const panel = (
    <div className="dvp-panel">
      <DevicesPanel threadId="preview-thread" />
    </div>
  );

  return (
    <div className={prepared.narrow ? "dvp-stage dvp-narrow" : "dvp-stage"}>
      {SETUP.has(frame) ? (
        <div className="dvp-panel">
          <div className="dev">
            <div className="dev-head">
              <div className="dev-head-title"><b>Devices</b><Pill>Planned</Pill></div>
            </div>
            <SetupHub
              platforms={prepared.platforms}
              deviceToOpen={prepared.deviceToOpen}
              initialStep={initialStep}
              initialPlatform={frame.includes("android") || frame.includes("windows") ? "android" : "ios"}
              initialInstall={installFor(frame)}
              host={frame === "device-setup-platform-windows" ? "other" : undefined}
              onOpen={() => {}}
              onClose={() => {}}
            />
          </div>
        </div>
      ) : VERSIONS.has(frame) ? (
        <div className="dvp-panel">
          <div className="dev">
            <div className="dev-head"><div className="dev-head-title"><b>Devices</b><Pill>Planned</Pill></div></div>
            <ToolVersions onBack={() => {}} initialOpenSteps={frame === "device-tool-versions-steps" ? ["ios-runtime"] : []} />
          </div>
        </div>
      ) : frame === "device-float" ? (
        <div className="dvp-thread">
          <p className="dev-note">The thread stays usable under the floating device.</p>
          <DevicesFloat threadId="preview-thread" />
        </div>
      ) : panel}
    </div>
  );
}

const subscribeHash = (notify: () => void) => {
  window.addEventListener("hashchange", notify);
  return () => window.removeEventListener("hashchange", notify);
};

export default function DevicesPreview() {
  const hash = useSyncExternalStore(subscribeHash, () => window.location.hash);
  const frame = frameOf(hash);
  return (
    <main className="dvp-page">
      <header className="dvp-head">
        <div>
          <b>Devices preview</b>
          <span className="dev-meta">dev only · mock service</span>
        </div>
        <label className="dvp-choose">
          <span>Frame</span>
          <select
            aria-label="Preview frame"
            value={frame}
            onChange={(event) => { window.location.hash = `#/preview/devices?frame=${encodeURIComponent(event.target.value)}`; }}
          >
            {FRAMES.map((id) => <option key={id} value={id}>{id}</option>)}
          </select>
        </label>
      </header>
      <Frame key={frame} frame={frame} />
    </main>
  );
}
