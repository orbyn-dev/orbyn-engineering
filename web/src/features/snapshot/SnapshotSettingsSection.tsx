import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { useUi } from "../../product/state/ui";
import { useSnapshot } from "./runtime";
import {
  BUILT_IN_EXCLUDED_APPS,
  MAX_EXCLUDED_APP_LENGTH,
  MAX_EXCLUDED_APPS,
  SNAPSHOT_SETUP_AVAILABLE,
  excludedAppProblem,
  shortcutKeys,
  shortcutText,
  type SnapShotShortcut,
  type SnapShotState,
} from "./types";

import "./snapshot.css";


const Row = ({ title, description, children }: { title: string; description: string; children: ReactNode }) => (
  <div className="setting-line">
    <div className="setting-line-text">
      <div className="setting-line-heading"><span className="setting-line-title">{title}</span></div>
      <span className="setting-line-desc">{description}</span>
    </div>
    <div className="setting-line-control">{children}</div>
  </div>
);

const Group = ({ title, children }: { title: ReactNode; children: ReactNode }) => (
  <section className="setting-group">
    <h2>{title}</h2>
    <div className="setting-list">{children}</div>
  </section>
);

const MODIFIER_KEYS = new Set(["Shift", "Control", "Alt", "Meta", "OS"]);

/** The physical key, so Shift+9 records 9 rather than its shifted glyph. */
const chordKey = (event: ReactKeyboardEvent<HTMLElement>): string => {
  const code = event.code;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^Numpad[0-9]$/.test(code)) return code.slice(6);
  return event.key.length === 1 ? event.key.toLowerCase() : event.key;
};

const Setup = ({ state, onOpen }: { state: SnapShotState; onOpen(): void }) => {
  const description = state.needs_setup === "screen_recording"
    ? "Screen Recording is not allowed. Allow it in System Settings, then try a capture."
    : state.needs_setup === "accessibility"
      ? "Accessibility is not allowed. Captures still work without text."
      : state.platform === "windows"
        ? "Windows asks for no permission. Check the shortcut and try a capture."
        : "Screen Recording is allowed. Check the shortcut and try a capture.";
  return (
    <Row title="Set up" description={description}>
      <Button small onClick={onOpen}>Set up Snap Shot…</Button>
    </Row>
  );
};

/** Records one chord: at least one modifier, Escape cancels, the description says so. */
function ShortcutRecorder({ platform, onDone, onCancel }: { platform: SnapShotState["platform"]; onDone(shortcut: SnapShotShortcut): void; onCancel(): void }) {
  const [error, setError] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.focus();
  }, []);
  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      onCancel();
      return;
    }
    if (MODIFIER_KEYS.has(event.key)) return;
    // macOS reads Cmd as mod and Ctrl as ctrl; Windows reads Ctrl as mod.
    const mod = platform === "macos" ? event.metaKey : event.ctrlKey;
    const shift = event.shiftKey;
    const alt = event.altKey;
    const ctrl = platform === "macos" ? event.ctrlKey : false;
    if (!mod && !shift && !alt && !ctrl) {
      setError("Press the keys you want, with at least one modifier. Escape cancels.");
      return;
    }
    setError(null);
    onDone({ kind: "chord", key: chordKey(event), mod, shift, alt, ctrl });
  };
  const preview: SnapShotShortcut = { kind: "chord", key: "9", mod: true, shift: true, alt: false, ctrl: false };
  return (
    <div className="snapshot-recorder" role="group" aria-label="Shortcut recorder" tabIndex={0} ref={ref} onKeyDown={onKeyDown}>
      <span className="pb-keys rec">
        {shortcutKeys(preview, platform).map((key) => <kbd key={key.aria}>{key.label}</kbd>)}
      </span>
      <span className="snapshot-recorder-line">Press the new shortcut</span>
      <Button small onClick={onCancel}>Cancel</Button>
      {error ? <p className="snapshot-recorder-error" role="alert">{error}</p> : null}
    </div>
  );
}

function ExclusionDialog({ onClose }: { onClose(): void }) {
  const excluded = useUi((state) => state.snapshotExcludedApps);
  const setPrefs = useUi((state) => state.setPrefs);
  const [value, setValue] = useState("");
  const [problem, setProblem] = useState<string | null>(null);
  const add = () => {
    const next = excludedAppProblem(value, excluded);
    if (next) {
      setProblem(next);
      return;
    }
    setPrefs({ snapshotExcludedApps: [...excluded, value.trim()] });
    setValue("");
    setProblem(null);
  };
  return (
    <Dialog title="Apps to skip" onClose={onClose} footer={<Button onClick={onClose}>Done</Button>}>
      <p className="snapshot-excluded-intro">Snap Shot skips these apps. It says so and keeps nothing.</p>
      <ul className="snapshot-excluded-list">
        {BUILT_IN_EXCLUDED_APPS.map((name) => (
          <li key={name}><span><b>{name}</b><small>Built in. Cannot be removed.</small></span><span aria-hidden>Built in</span></li>
        ))}
      </ul>
      <div className="snapshot-excluded-added">Added by you</div>
      {excluded.length === 0 ? <p className="snapshot-excluded-empty">No apps added.</p> : (
        <ul className="snapshot-excluded-list">
          {excluded.map((id) => (
            <li key={id}>
              <span><b>{id}</b><small>You can remove it.</small></span>
              <Button small onClick={() => setPrefs({ snapshotExcludedApps: excluded.filter((entry) => entry !== id) })}>Remove {id}</Button>
            </li>
          ))}
        </ul>
      )}
      <div className="snapshot-excluded-add">
        <input aria-label="App id" placeholder="App id, for example com.example.app" value={value}
          maxLength={MAX_EXCLUDED_APP_LENGTH + 1} onChange={(event) => { setValue(event.target.value); setProblem(null); }} />
        <Button onClick={add}>Add</Button>
      </div>
      <p className="snapshot-excluded-count">{excluded.length} of {MAX_EXCLUDED_APPS} added. The built-in apps stay on the list.</p>
      {problem ? <p className="snapshot-excluded-error" role="alert">{problem}</p> : null}
    </Dialog>
  );
}

/** Settings › General › Desktop app › Snap Shot (K49). Hidden entirely when the shell is absent or unsupported. */
export function SnapshotSettingsSection() {
  const status = useSnapshot((state) => state.status);
  const state = useSnapshot((state) => state.state);
  const waitingFor = useSnapshot((state) => state.waitingFor);
  const enabled = useUi((user) => user.snapshotEnabled);
  const shortcut = useUi((user) => user.snapshotShortcut);
  const includeAccessibility = useUi((user) => user.snapshotIncludeAccessibility);
  const playSound = useUi((user) => user.snapshotPlaySound);
  const sound = useUi((user) => user.snapshotSound);
  const flash = useUi((user) => user.snapshotFlash);
  const animations = useUi((user) => user.snapshotAnimations);
  const excluded = useUi((user) => user.snapshotExcludedApps);
  const setPrefs = useUi((user) => user.setPrefs);
  const openSetup = useSnapshot((store) => store.openSetup);
  const setupAction = useSnapshot((store) => store.setupAction);
  const closeSetup = useSnapshot((store) => store.closeSetup);
  const [recording, setRecording] = useState(false);
  const [editingExclusions, setEditingExclusions] = useState(false);

  if (status !== "ready" || !state?.supported) return null;
  const mac = state.platform === "macos";
  const accessibilityMissing = includeAccessibility && !state.permissions.accessibility;
  const waiting = waitingFor === "accessibility" && accessibilityMissing;

  const setEnabled = (next: boolean) => {
    setPrefs({ snapshotEnabled: next });
    if (next) {
      setRecording(false);
      openSetup(state.needs_setup === "accessibility" ? "accessibility" : "all");
    } else {
      closeSetup();
    }
  };
  const setAccessibility = (next: boolean) => {
    setPrefs({ snapshotIncludeAccessibility: next });
    if (next && !state.permissions.accessibility) openSetup("accessibility");
  };

  return (
    <Group title={<><span>Snap Shot</span><span className="set-pill planned">Planned</span></>}>
      <Row
        title="Snap Shot"
        description={enabled
          ? "Capture the window in front of Orbyn and add it to a draft. You send it yourself."
          : `Off. Orbyn registers no shortcut and asks ${mac ? "macOS" : "Windows"} for nothing until you turn this on.`}
      >
        <input type="checkbox" role="switch" aria-label="Snap Shot" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} />
      </Row>

      {enabled ? (
        <>
          {recording ? (
            <Row title="Shortcut" description="Press the keys you want, with at least one modifier. Escape cancels.">
              <ShortcutRecorder
                platform={state.platform}
                onDone={(next) => { setPrefs({ snapshotShortcut: next }); setRecording(false); }}
                onCancel={() => setRecording(false)}
              />
            </Row>
          ) : (
            <Row title="Shortcut" description="Press it in any app to capture. At least one modifier key.">
              <span className="pb-keys">
                {shortcutKeys(shortcut, state.platform).map((key) => <kbd key={key.aria}>{key.label}</kbd>)}
              </span>
              {!state.shortcut_registered ? <Button small onClick={() => void setupAction("retry_shortcut")}>Try again</Button> : null}
              <Button small tone="quiet" onClick={() => setRecording(true)}>Change</Button>
            </Row>
          )}
          {recording ? (
            <Row title="Other kinds" description="Both Shift keys, or one modifier pair. Planned.">
              <Button small disabled>Both Shift keys</Button>
              <Button small disabled>Modifier pair</Button>
              <span className="set-pill planned">Planned</span>
            </Row>
          ) : null}
          {!state.shortcut_registered ? (
            <p className="banner snapshot-error">
              <b>{state.shortcut_message ?? `Another app uses ${shortcutText(shortcut, state.platform)}.`}</b>{" "}
              Snap Shot cannot start from the keyboard until you choose another shortcut or that app lets go of it. The button in the composer still works.
            </p>
          ) : null}

          <Row
            title="Include accessibility text"
            description={mac
              ? "Also read the window's text and attach it. Off by default: it sends what is in another app's window. Needs Accessibility."
              : "Planned on Windows. Reading a window's text there needs UI Automation, which is not built."}
          >
            <input type="checkbox" role="switch" aria-label="Include accessibility text" checked={mac && includeAccessibility}
              disabled={!mac} onChange={(event) => setAccessibility(event.target.checked)} />
            {!mac ? <span className="set-pill planned">Planned</span> : null}
          </Row>
          {accessibilityMissing ? (
            <p className="banner snapshot-warn">
              Accessibility is not allowed for Orbyn, so no text is read. Captures still work without it.
              {waiting ? <span className="snapshot-waiting"><i className="spin" aria-label="Waiting" />Waiting</span> : null}
              {SNAPSHOT_SETUP_AVAILABLE ? <Button small tone="primary" onClick={() => void setupAction("allow_accessibility")}>Allow Accessibility</Button> : null}
            </p>
          ) : null}

          <Row title="Sound" description="Played when a window is captured.">
            <select aria-label="Sound" value={sound} onChange={(event) => setPrefs({ snapshotSound: event.target.value === "camera-shutter" ? "camera-shutter" : "soft-pop" })}>
              <option value="soft-pop">Soft pop</option>
              <option value="camera-shutter">Shutter</option>
            </select>
            <input type="checkbox" role="switch" aria-label="Play a sound" checked={playSound} onChange={(event) => setPrefs({ snapshotPlaySound: event.target.checked })} />
          </Row>
          <Row title="Flash" description="Flash the screen once when a window is captured.">
            <input type="checkbox" role="switch" aria-label="Flash" checked={flash} onChange={(event) => setPrefs({ snapshotFlash: event.target.checked })} />
          </Row>
          <Row title="Animations" description="Animate the capture into the draft. Off when Reduce motion is on.">
            <input type="checkbox" role="switch" aria-label="Animations" checked={animations} onChange={(event) => setPrefs({ snapshotAnimations: event.target.checked })} />
          </Row>
          <Row title="Apps to skip" description="Orbyn and password managers are skipped. Add other apps here.">
            <span className="snapshot-added-count">{excluded.length === 0 ? "None added" : `${excluded.length} added`}</span>
            <Button small tone="quiet" onClick={() => setEditingExclusions(true)}>Edit list…</Button>
          </Row>
          {SNAPSHOT_SETUP_AVAILABLE ? <Setup state={state} onOpen={() => openSetup("all")} /> : null}
        </>
      ) : null}

      {editingExclusions ? <ExclusionDialog onClose={() => setEditingExclusions(false)} /> : null}
    </Group>
  );
}
