import { Smartphone } from "lucide-react";
import { useState } from "react";
import { useAsync } from "../../../lib/useAsync";
import { Pill } from "../../../ui/controls";
import { plain } from "../model";
import {
  devicesPlannedApi,
  usePlannedApiVersion,
  type AgentAccessState,
  type AgentDeviceGrant,
  type AgentDeviceRequest,
  type AgentRequestDecision,
} from "../planned";
import "./agent.css";

const OFF = "Off. Agents cannot use devices.";
const ON = "On. An agent can use a device when a run asks.";
const PLANNED = "Agent access is not available on this service yet; it always stays off.";

/**
 * The person's own switch (H27). Off is the default and the only resting state without a service: reading the Planned
 * client never turns it on, and only this switch's click asks the service to. A refusal is shown as the service's
 * plain sentence and the switch resolves back off.
 */
export function AgentAccessSetting() {
  const seen = usePlannedApiVersion();
  const read = useAsync(`agent-access:${seen}`, (signal) => devicesPlannedApi().agentAccess(signal));
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  const [refused, setRefused] = useState("");
  const state: AgentAccessState | null = read.status === "ready" ? read.data : null;
  const planned = read.status === "ready" && state === null;
  const enabled = state?.enabled === true;
  const status = planned || refused ? PLANNED : enabled ? ON : OFF;

  const toggle = async () => {
    if (busy || planned || read.status !== "ready") return;
    const next = !enabled;
    setBusy(true);
    setFailure("");
    setRefused("");
    try {
      const answer = await devicesPlannedApi().setAgentAccess(next);
      if (answer === null) {
        setRefused(PLANNED);
        return;
      }
      read.setData(answer);
      if (next && !answer.enabled) setRefused(answer.reason ?? "Agent access always stays off on this service.");
    } catch (error) {
      setFailure(error instanceof Error ? plain(error.message, 200) : "Couldn’t change agent access.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="ag-access" aria-label="Agent access">
      <div className="ag-access-head">
        <div className="ag-access-label">
          <b>Let agents use devices</b>
          <p className="ag-note">Agents can open a device, tap, type and take screenshots. They cannot install apps or change device settings without asking.</p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={enabled}
          aria-label="Let agents use devices"
          className="dev-switch"
          disabled={busy || planned || read.status !== "ready"}
          onClick={() => void toggle()}
        >
          <span className="dev-switch-knob" aria-hidden />
        </button>
      </div>
      <p className="ag-status" role="status">
        {planned || refused ? <><Pill>Planned</Pill> <span>{refused || PLANNED}</span></> : <span>{status}</span>}
      </p>
      {failure ? <p className="ag-error" role="alert">{failure}</p> : null}
    </section>
  );
}

/**
 * One agent's request to use one device. It waits for an answer and never grants anything by being drawn: Deny,
 * Allow once and Allow for this thread are the only paths. The Planned state disables all three.
 */
export function AgentPermissionCard({ request, onAnswer, planned = false, busy = false }: {
  request: AgentDeviceRequest;
  onAnswer(decision: AgentRequestDecision): void;
  planned?: boolean;
  busy?: boolean;
}) {
  return (
    <div className="ag-ask" role="group" aria-label="Device permission request">
      <span className="ag-lab"><span className="ag-dot" aria-hidden />Needs you</span>
      <p className="ag-question">
        Let <b>{plain(request.agent, 60)}</b> use the <b>{plain(request.device, 60)}</b> simulator?
      </p>
      <p className="ag-why">It can open the device, tap, type and take screenshots. It cannot install apps without asking again.</p>
      <div className="ag-actions">
        {planned ? <Pill>Planned</Pill> : null}
        <button type="button" className="ag-ghost" disabled={planned || busy} onClick={() => onAnswer("deny")}>Deny</button>
        <button type="button" className="ag-ghost" disabled={planned || busy} onClick={() => onAnswer("once")}>Allow once</button>
        <button type="button" className="ag-primary" disabled={planned || busy} onClick={() => onAnswer("thread")}>Allow for this thread</button>
      </div>
    </div>
  );
}

/**
 * The thread header's running-as chip (device-agent-permission-granted). Its tooltip says who and until when. It is
 * shown only while a grant exists; clicking it opens the Device tab. Nothing here changes the grant.
 */
export function AgentDeviceChip({ grant, onOpen }: { grant: AgentDeviceGrant; onOpen(): void }) {
  const until = grant.scope === "thread" ? "this thread" : "this once";
  return (
    <button
      type="button"
      className="ag-chip"
      aria-label="Device"
      title={`${plain(grant.agent, 60)} · ${until}`}
      onClick={onOpen}
    >
      <Smartphone size={12} aria-hidden />
      Device
    </button>
  );
}
