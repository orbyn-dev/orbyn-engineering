import { Circle, Loader2, RefreshCw, Shield, Smartphone, X } from "lucide-react";
import { useEffect, useState } from "react";
import { IconButton, Pill } from "../../ui/controls";
import { useAsync } from "../../lib/useAsync";
import { useUi } from "../../product/state/ui";
import { DevicesPanel } from "./DevicesPanel";
import { setDevicesApi } from "./api";
import type { Device } from "./contract";
import { stateLabel } from "./model";
import { devicesPlannedApi, setDevicesPlannedApi, usePlannedApiVersion, type AgentAccessState, type AgentRequestDecision } from "./planned";
import { AgentDeviceChip, AgentPermissionCard } from "./agent/AgentDevice";
import { DeviceToolsDrawer, DeviceToolsPlatforms, type ToolTab } from "./tools/DeviceTools";
import "./DevicesToolsPreview.css";

/** The preview frames this file owns. `DevicesPreview` adds them to its frame list in one line. */
export const TOOLS_FRAMES = [
  "device-tools-appearance",
  "device-tools-text",
  "device-tools-accessibility",
  "device-tools-location",
  "device-tools-permissions",
  "device-tools-push",
  "device-tools-push-android",
  "device-tools-platforms",
  "device-agent-permission",
  "device-agent-permission-lead",
  "device-agent-permission-granted",
  "device-agent-permission-revoked",
  "device-agent-off",
] as const;

const TOOL_TABS: Record<string, ToolTab> = {
  "device-tools-appearance": "appearance",
  "device-tools-text": "text",
  "device-tools-accessibility": "accessibility",
  "device-tools-location": "location",
  "device-tools-permissions": "permissions",
  "device-tools-push": "push",
  "device-tools-push-android": "push",
};

const isAgentFrame = (frame: string): boolean => frame.startsWith("device-agent-");

interface Seed {
  access: boolean;
  request: AgentAccessState["request"];
  grant: AgentAccessState["grant"];
}

function agentSeed(frame: string, device: Device): Seed {
  // These are the frame's own states: the permission frames show a person who has already turned access on. The default
  // mock stays off; no render calls setAgentAccess, so drawing a frame never grants anything.
  if (frame === "device-agent-permission") {
    return { access: true, request: { id: "req-1", agent: "Claude Code", device: device.name, thread: "preview-thread" }, grant: null };
  }
  if (frame === "device-agent-permission-lead") {
    return { access: true, request: null, grant: { agent: "Claude Code", device: device.name, scope: "thread", thread: "preview-thread", by: "lead", mode: "Autopilot Full" } };
  }
  if (frame === "device-agent-permission-granted") {
    return { access: true, request: null, grant: { agent: "Claude Code", device: device.name, scope: "thread", thread: "preview-thread", by: "person" } };
  }
  return { access: false, request: null, grant: null };
}

/**
 * The dev-only frames for H26 and H27. It installs the device and Planned mocks and draws the real panel with the
 * drawer, or the thread's permission states. Nothing here is imported in a production build: the whole module sits
 * behind `__ORBYN_PREVIEWS__` through `DevicesPreview`.
 */
export function DevicesToolsPreview({ frame }: { frame: string }) {
  const [ready, setReady] = useState(false);
  const [device, setDevice] = useState<Device | null>(null);

  useEffect(() => {
    let live = true;
    setReady(false);
    void import("../../../mock/devices").then((m) => {
      if (!live) return;
      const chosen = frame === "device-tools-push-android" ? m.stateFixture("booted", "android") : m.stateFixture("booted");
      setDevice(chosen);
      setDevicesApi(m.createDevicesMock({ devices: [chosen] }));
      if (isAgentFrame(frame)) {
        const seed = agentSeed(frame, chosen);
        setDevicesPlannedApi(m.createDevicesPlannedMock({ access: seed.access, agentRequest: seed.request, agentGrant: seed.grant }));
      } else {
        setDevicesPlannedApi(m.createDevicesPlannedMock());
      }
      useUi.setState({ devices: { open: [chosen.id], active: chosen.id, names: {}, float: null } });
      setReady(true);
    });
    return () => {
      live = false;
      setDevicesApi(null);
      setDevicesPlannedApi(null);
    };
  }, [frame]);

  if (!ready || !device) return null;
  if (frame === "device-tools-platforms") return <PlatformsFrame />;
  if (isAgentFrame(frame)) return <AgentFrame frame={frame} device={device} />;
  return <ToolsFrame frame={frame} device={device} />;
}

function ToolsFrame({ frame, device }: { frame: string; device: Device }) {
  return (
    <div className="dtp-stage dtp-stage-tools">
      <div className="dvp-panel dtp-tools-panel">
        <DevicesPanel threadId="preview-thread" />
        <DeviceToolsDrawer device={device} initialTab={TOOL_TABS[frame] ?? "appearance"} onClose={() => {}} />
      </div>
    </div>
  );
}

function PlatformsFrame() {
  return (
    <div className="dtp-stage">
      <div className="dvp-panel dtp-platform-panel">
        <div className="dev">
          <div className="dev-head">
            <div className="dev-head-title"><b>Device</b><Pill>Planned</Pill></div>
            <div className="dev-head-actions">
              <IconButton label="Refresh devices" onClick={() => {}} disabled><RefreshCw size={14} aria-hidden /></IconButton>
            </div>
          </div>
          <DeviceToolsPlatforms />
        </div>
      </div>
    </div>
  );
}

function AgentFrame({ frame, device }: { frame: string; device: Device }) {
  return (
    <div className="dtp-stage dtp-stage-agent">
      <div className="dvp-panel dtp-agent-panel">
        <DevicesPanel threadId="preview-thread" />
      </div>
      <AgentThread frame={frame} device={device} />
    </div>
  );
}

function AgentThread({ frame, device }: { frame: string; device: Device }) {
  const seen = usePlannedApiVersion();
  const read = useAsync(`preview-agent:${seen}:${frame}`, (signal) => devicesPlannedApi().agentAccess(signal));
  const state = read.status === "ready" ? read.data : null;
  const planned = read.status === "ready" && state === null;
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  const grant = state?.grant ?? null;
  const request = state?.request ?? null;
  const permissionFrame = frame === "device-agent-permission";

  const answer = async (decision: AgentRequestDecision) => {
    if (!request || busy) return;
    setBusy(true);
    setFailure("");
    try {
      const next = await devicesPlannedApi().answerAgentRequest(request.id, decision);
      if (next === null) setFailure("Answering a device request is Planned. The service has no such route yet.");
      else read.setData(next);
    } catch (error) {
      setFailure(error instanceof Error ? error.message : "Couldn’t answer the request.");
    } finally {
      setBusy(false);
    }
  };

  const waiting = permissionFrame && !grant;
  const status = grant ? "running" : waiting ? "waiting" : null;

  return (
    <div className="dtp-thread">
      <header className="dtp-head">
        <b>Fix login redirect</b>
        <span className="dtp-branch">fix/login</span>
        {grant && (frame === "device-agent-permission-granted" || permissionFrame) ? <AgentDeviceChip grant={grant} onOpen={() => {}} /> : null}
        {status === "waiting" ? <span className="dtp-status dtp-wait"><span className="dtp-dot" aria-hidden />Waiting for you</span> : null}
        {status === "running" ? <span className="dtp-status"><Loader2 size={12} className="spin" aria-hidden />Running</span> : null}
      </header>
      <div className="dtp-body">
        <p className="dtp-you">{permissionFrame || frame === "device-agent-permission-lead" || frame === "device-agent-permission-granted" ? "Open the sign-in screen on the iPhone simulator and check that the keyboard does not cover the button." : "Open the sign-in screen on the iPhone simulator."}</p>
        {permissionFrame ? (
          request ? <AgentPermissionCard request={request} onAnswer={(decision) => void answer(decision)} planned={planned} busy={busy} /> : grant ? (
            <GrantedSteps device={device.name} />
          ) : (
            <p className="dtp-note">No device request is waiting.</p>
          )
        ) : frame === "device-agent-permission-lead" ? (
          <>
            <div className="dtp-steps">
              <p className="dtp-step"><Shield size={13} aria-hidden /><span>Lead allowed <b>{device.name}</b></span><span className="dtp-t3">for this thread</span><span className="dtp-sep" aria-hidden>·</span><span className="dtp-t3">Autopilot Full</span><span className="dtp-sep" aria-hidden>·</span><span className="dtp-t3">Agent access is on</span></p>
              <p className="dtp-step"><Smartphone size={13} aria-hidden /><span>Opened</span><span className="dtp-t2">{`${device.name} · ${device.platform === "ios" ? "iOS" : "Android"} ${device.version}`}</span></p>
            </div>
          </>
        ) : frame === "device-agent-permission-granted" ? (
          <>
            <GrantedSteps device={device.name} />
            <p className="dtp-say">The keyboard covers the Sign in button on this screen size. I will scroll the form up when the keyboard opens.</p>
          </>
        ) : frame === "device-agent-permission-revoked" ? (
          <>
            <div className="dtp-steps">
              <p className="dtp-step"><Smartphone size={13} aria-hidden /><span>Opened</span><span className="dtp-t2">{device.name}</span></p>
              <p className="dtp-step dtp-failed"><X size={13} aria-hidden /><span className="dtp-t2">device_screenshot</span><span className="dtp-t3">Access to devices was turned off</span></p>
            </div>
            <p className="dtp-say">I lost access to the device, so I cannot take a screenshot. Turn access on in the Device tab if you want me to continue.</p>
          </>
        ) : frame === "device-agent-off" ? (
          <>
            <div className="dtp-steps">
              <p className="dtp-step dtp-failed"><X size={13} aria-hidden /><span className="dtp-t2">device_open</span><span className="dtp-t3">Agent access is off</span></p>
            </div>
            <p className="dtp-say">Devices are off for agents, so I cannot open the simulator. Turn on Agent access in the Device tab, or open the device yourself and I will read screenshots you attach.</p>
          </>
        ) : null}
        {failure ? <p className="dtp-error" role="alert">{failure}</p> : null}
        <p className="dtp-note dtp-device-note">{`${device.name} · ${stateLabel(device.state)}`}</p>
      </div>
      <input className="dtp-composer" aria-label="Thread reply" readOnly placeholder={permissionFrame ? "Type a reply to decline with feedback" : "Ask anything. @ files, $ skills, / commands"} />
    </div>
  );
}

function GrantedSteps({ device }: { device: string }) {
  return (
    <div className="dtp-steps">
      <p className="dtp-step"><Smartphone size={13} aria-hidden /><span>Opened</span><span className="dtp-t2">{`${device} · iOS 18.2`}</span></p>
      <p className="dtp-step"><Circle size={11} aria-hidden /><span>Took a screenshot of the device</span></p>
      <p className="dtp-step"><Circle size={11} aria-hidden /><span>Tapped</span><span className="dtp-t2">Email</span></p>
    </div>
  );
}
