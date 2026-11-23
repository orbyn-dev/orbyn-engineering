import { Bell, Eye } from "lucide-react";
import { useEffect, useState } from "react";
import { Button, Pill } from "../../../ui/controls";
import { plain } from "../model";
import type { DeviceToolChange, DeviceToolsState, PermissionDecision, PushOutcome, ToolsAccessibilityElement, ToolsPermission } from "../planned";

/** Every drawer tab draws the same two lines above its controls: the labels stay until a real device confirms them. */
export function NotVerified() {
  return (
    <div className="dt-verify">
      <Pill>Not verified</Pill>
      <span>Not seen on a real device yet</span>
    </div>
  );
}

function Switch({ label, on, disabled, title, onChange }: { label: string; on: boolean; disabled?: boolean; title?: string; onChange(on: boolean): void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      className="dev-switch"
      disabled={disabled}
      title={title}
      onClick={() => onChange(!on)}
    >
      <span className="dev-switch-knob" aria-hidden />
    </button>
  );
}

interface Props {
  state: DeviceToolsState;
  busy: boolean;
  onChange(change: DeviceToolChange): void;
  onSendPush(payload: string): Promise<PushOutcome | null>;
}

export function AppearancePane({ state, busy, onChange }: Props) {
  return (
    <>
      <NotVerified />
      <div className="dt-row">
        <div className="dt-row-label"><b>Appearance</b><small>The device’s own setting</small></div>
        <div className="dt-seg" role="group" aria-label="Appearance">
          {(["light", "dark"] as const).map((mode) => (
            <button key={mode} type="button" aria-pressed={state.appearance.mode === mode} disabled={busy} onClick={() => onChange({ type: "appearance", mode })}>
              {mode === "light" ? "Light" : "Dark"}
            </button>
          ))}
        </div>
      </div>
      <div className="dt-row">
        <div className="dt-row-label"><b>Increase contrast</b></div>
        <Switch label="Increase contrast" on={state.appearance.increaseContrast} disabled={busy} onChange={(on) => onChange({ type: "increase_contrast", on })} />
      </div>
      <div className="dt-row">
        <div className="dt-row-label"><b>Reduce motion</b></div>
        <Switch label="Reduce motion" on={state.appearance.reduceMotion} disabled={busy} onChange={(on) => onChange({ type: "reduce_motion", on })} />
      </div>
    </>
  );
}

export function TextPane({ state, busy, onChange }: Props) {
  const text = state.text;
  const reset = Math.max(1, Math.round(text.total / 4));
  return (
    <>
      <NotVerified />
      <div className="dt-row">
        <div className="dt-row-label"><b>Text size</b><small>{`${plain(text.name, 40)} · step ${text.step} of ${text.total}`}</small></div>
        <div className="dt-seg" role="group" aria-label="Text size">
          <button type="button" aria-label="Smaller" disabled={busy || text.step <= 1} onClick={() => onChange({ type: "text_size", step: text.step - 1 })}>−</button>
          <button type="button" aria-label="Default" aria-pressed={text.step === reset} disabled={busy} onClick={() => onChange({ type: "text_size", step: reset })}>Default</button>
          <button type="button" aria-label="Larger" disabled={busy || text.step >= text.total} onClick={() => onChange({ type: "text_size", step: text.step + 1 })}>+</button>
        </div>
      </div>
      <div className="dt-row">
        <div className="dt-row-label"><b>Bold text</b></div>
        <Switch label="Bold text" on={text.bold} disabled={busy} onChange={(on) => onChange({ type: "bold", on })} />
      </div>
    </>
  );
}

export function AccessibilityPane({ state, busy, live, onChange }: Props & { live: boolean }) {
  const [selected, setSelected] = useState<string | null>(null);
  const reason = live ? null : "Outlines need a live screen.";
  return (
    <>
      <NotVerified />
      <div className="dt-row">
        <div className="dt-row-label">
          <b>Show elements</b>
          <small>{reason ?? "Outline every element the device exposes"}</small>
        </div>
        <Switch
          label="Show elements"
          title={reason ?? undefined}
          on={state.accessibility.showElements}
          disabled={busy || !live}
          onChange={(on) => onChange({ type: "show_elements", on })}
        />
      </div>
      {live ? state.accessibility.elements.map((element) => (
        <ElementRow key={element.id} element={element} selected={selected === element.id} onSelect={() => setSelected(element.id)} />
      )) : <p className="dt-note">No screen is live.</p>}
    </>
  );
}

function ElementRow({ element, selected, onSelect }: { element: ToolsAccessibilityElement; selected: boolean; onSelect(): void }) {
  return (
    <button type="button" className="dt-row dt-row-button" aria-pressed={selected} onClick={onSelect}>
      <span className="dt-row-icon" aria-hidden><Eye size={14} /></span>
      <span className="dt-row-label">
        <b>{`${plain(element.label, 60)} · ${plain(element.role, 30)}`}</b>
        <small>{plain(element.detail, 160)}</small>
      </span>
    </button>
  );
}

export function LocationPane({ state, busy, onChange }: Props) {
  const [coordinates, setCoordinates] = useState(state.location.coordinates);
  const [place, setPlace] = useState(state.location.place ?? "None");
  useEffect(() => {
    setCoordinates(state.location.coordinates);
    setPlace(state.location.place ?? "None");
  }, [state.location.coordinates, state.location.place]);
  const set = () => onChange({ type: "location", place: place === "None" ? null : place, coordinates: plain(coordinates, 80) });
  return (
    <>
      <NotVerified />
      <div className="dt-row">
        <div className="dt-row-label"><b>Place</b><small>Used by maps and location APIs on the device</small></div>
        <select
          className="dt-select"
          aria-label="Place"
          value={place}
          disabled={busy}
          onChange={(event) => {
            const next = event.target.value;
            setPlace(next);
            onChange({ type: "location", place: next === "None" ? null : next, coordinates: next === "None" ? "" : coordinates });
          }}
        >
          {state.location.places.map((one) => <option key={one} value={one}>{one}</option>)}
        </select>
      </div>
      <div className="dt-row">
        <div className="dt-row-label">
          <b>Coordinates</b>
        </div>
        <div className="dt-row-side">
          <input
            className="dt-input"
            aria-label="Coordinates"
            value={coordinates}
            maxLength={80}
            disabled={busy}
            onChange={(event) => setCoordinates(event.target.value)}
          />
          <Button small tone="quiet" disabled={busy} onClick={set}>Set</Button>
        </div>
      </div>
    </>
  );
}

export function PermissionsPane({ state, busy, onChange }: Props) {
  return (
    <>
      <NotVerified />
      {state.permissions.map((permission) => <PermissionRow key={permission.id} permission={permission} busy={busy} onChange={onChange} />)}
    </>
  );
}

function PermissionRow({ permission, busy, onChange }: { permission: ToolsPermission; busy: boolean; onChange(change: DeviceToolChange): void }) {
  const decisions: readonly PermissionDecision[] = ["ask", "allow", "deny"];
  return (
    <div className="dt-row" role="group" aria-label={permission.name}>
      <div className="dt-row-label">
        <b>{plain(permission.name, 40)}</b>
        <small>{permission.askedOnce ? `${plain(permission.app, 40)} · asked once` : plain(permission.app, 40)}</small>
      </div>
      <div className="dt-seg" role="group" aria-label={`${permission.name} permission`}>
        {decisions.map((decision) => (
          <button
            key={decision}
            type="button"
            aria-pressed={permission.decision === decision}
            title={decision === "ask" ? "Reset: the app asks again" : undefined}
            disabled={busy}
            onClick={() => onChange({ type: "permission", id: permission.id, decision })}
          >
            {decision === "ask" ? "Ask" : decision === "allow" ? "Allow" : "Deny"}
          </button>
        ))}
      </div>
    </div>
  );
}

/** iOS keeps the editor; Android shows the same tab with its plain reason, as device-tools-push-android draws. */
export function PushPaneFor({ platform, ...props }: Props & { platform: "android" | "ios" }) {
  if (platform === "ios") return <PushEditor {...props} />;
  return <PushAndroid {...props} />;
}

function PushEditor({ state, busy, onChange, onSendPush }: Props) {
  const [payload, setPayload] = useState(state.push.payload);
  const [app, setApp] = useState(state.push.app);
  const [error, setError] = useState("");
  const [sending, setSending] = useState(false);
  useEffect(() => { setPayload(state.push.payload); }, [state.push.payload]);
  useEffect(() => { setApp(state.push.app); }, [state.push.app]);
  const disabled = busy || sending;
  const send = async () => {
    if (disabled) return;
    setSending(true);
    setError("");
    try {
      const answer = await onSendPush(payload);
      if (answer === null) setError("Sending a test notification is Planned. The service has no such route yet.");
      else if (!answer.ok) setError(plain(answer.error ?? "The device did not accept the notification.", 200));
    } catch (failure) {
      setError(failure instanceof Error ? plain(failure.message, 200) : "The device did not accept the notification.");
    } finally {
      setSending(false);
    }
  };
  return (
    <>
      <NotVerified />
      <textarea
        className="dt-textarea"
        aria-label="Notification payload"
        value={payload}
        rows={4}
        maxLength={4000}
        disabled={disabled}
        onChange={(event) => setPayload(event.target.value)}
      />
      <div className="dt-push-row">
        <select className="dt-select" aria-label="App" value={app} disabled={disabled} onChange={(event) => { setApp(event.target.value); onChange({ type: "push_app", app: event.target.value }); }}>
          {state.push.apps.map((one) => <option key={one} value={one}>{one}</option>)}
        </select>
        <span className="dt-spacer" />
        <Button small disabled={disabled} onClick={() => void send()}>Send</Button>
      </div>
      {error ? <p className="dt-error" role="alert">{error}</p> : null}
    </>
  );
}

function PushAndroid({ state, onChange }: Props) {
  return (
    <>
      <NotVerified />
      <div className="dt-row">
        <span className="dt-row-icon" aria-hidden><Bell size={14} /></span>
        <div className="dt-row-label">
          <b>Push notification</b>
          <small>Sending a test notification to the app</small>
        </div>
        <span className="dt-status">Not available on Android</span>
      </div>
      <div className="dt-push-row">
        <select className="dt-select" aria-label="App" value={state.push.app} disabled onChange={(event) => onChange({ type: "push_app", app: event.target.value })}>
          {state.push.apps.map((one) => <option key={one} value={one}>{one}</option>)}
        </select>
        <span className="dt-spacer" />
        <Button small disabled>Send</Button>
      </div>
    </>
  );
}
