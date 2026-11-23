import { useEffect, type ReactNode } from "react";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { useUi } from "../../product/state/ui";
import { useSnapshot } from "./runtime";
import { SnapshotDraftChip, SnapshotWorks } from "./SnapshotChip";
import { shortcutKeys, shortcutText, SNAPSHOT_ACCESSIBILITY_SETUP_AVAILABLE, SNAPSHOT_SETUP_AVAILABLE } from "./types";
import "./snapshot.css";

const Pill = ({ children, kind = "" }: { children: ReactNode; kind?: string }) => (
  <span className={`set-pill ${kind}`.trim()}>{children}</span>
);

const Waiting = () => <span className="snapshot-waiting"><i className="spin" aria-label="Waiting" />Waiting</span>;

function Row({ title, description, children }: { title: string; description: string; children: ReactNode }) {
  return (
    <div className="snapshot-setup-row">
      <div><b>{title}</b><small>{description}</small></div>
      <div className="snapshot-setup-control">{children}</div>
    </div>
  );
}

/** The setup dialog (H34): it opens System Settings and reads the answer back; it never grants anything itself. */
export function SnapshotSetupDialog() {
  const open = useSnapshot((state) => state.setupOpen);
  const state = useSnapshot((state) => state.state);
  const focus = useSnapshot((state) => state.setupFocus);
  const waitingFor = useSnapshot((state) => state.waitingFor);
  const worked = useSnapshot((state) => state.testCaptureWorked);
  const testDraft = useSnapshot((state) => state.testDraft);
  const clearTestDraft = useSnapshot((state) => state.clearTestDraft);
  const removeTestText = useSnapshot((state) => state.removeTestText);
  const discard = useSnapshot((state) => state.discard);
  const action = useSnapshot((state) => state.setupAction);
  const test = useSnapshot((state) => state.testCapture);
  const close = useSnapshot((state) => state.closeSetup);
  const refresh = useSnapshot((state) => state.refreshState);
  const chord = useUi((user) => user.snapshotShortcut);
  const includeText = useUi((user) => user.snapshotIncludeAccessibility);
  const setPrefs = useUi((user) => user.setPrefs);
  const platform = state?.platform;
  const screenRecording = state?.permissions.screen_recording ?? false;
  const waitingForScreen = waitingFor === "screen_recording" && !screenRecording;

  useEffect(() => {
    if (!open || platform !== "macos" || !waitingForScreen) return;
    const timer = setInterval(() => {
      if (!document.hidden) void refresh();
    }, 2000);
    return () => clearInterval(timer);
  }, [open, platform, waitingForScreen, refresh]);

  useEffect(() => {
    if (!open) return;
    const read = () => void refresh();
    window.addEventListener("focus", read);
    return () => window.removeEventListener("focus", read);
  }, [open, refresh]);

  if (!SNAPSHOT_SETUP_AVAILABLE || !open || !state || !state.supported) return null;
  const mac = state.platform === "macos";
  const accessibility = state.permissions.accessibility;
  const waitingForAccess = waitingFor === "accessibility" && !accessibility;
  const keys = shortcutKeys(chord, state.platform);
  const testBlocked = mac && !screenRecording;

  return (
    <Dialog
      title="Set up Snap Shot"
      onClose={close}
      footer={(
        <>
          {mac && SNAPSHOT_ACCESSIBILITY_SETUP_AVAILABLE && focus === "accessibility" ? <Button onClick={() => { setPrefs({ snapshotIncludeAccessibility: false }); close(); }}>Turn off the text</Button> : null}
          {mac ? <Button onClick={close}>Not now</Button> : null}
          {mac ? <Button tone="primary" disabled={waitingForScreen || waitingForAccess} onClick={close}>Done</Button> : <Button tone="primary" onClick={close}>Done</Button>}
        </>
      )}
    >
      <p className="snapshot-setup-intro">
        {mac
          ? "Snap Shot captures the window in front of Orbyn and adds it to your draft. macOS has to allow Screen Recording; Orbyn only opens System Settings and checks the answer."
          : "Snap Shot captures the window in front of Orbyn and adds it to your draft."}
      </p>
      {mac ? (
        <>
          <Row
            title="Screen Recording"
            description={screenRecording
              ? "Granted by macOS"
              : waitingForScreen
                ? "System Settings is open. Turn Orbyn on under Privacy & Security, then come back. Orbyn checks again when you do."
                : "Not granted."}
          >
            {screenRecording ? <Pill kind="good">Granted</Pill> : waitingForScreen ? <Waiting /> : (
              <>
                <Pill>Not granted</Pill>
                <Button small tone="primary" onClick={() => void action("allow_screen_recording")}>Allow Screen Recording</Button>
              </>
            )}
          </Row>
          {SNAPSHOT_ACCESSIBILITY_SETUP_AVAILABLE ? (
            <Row
              title="Accessibility"
              description={accessibility
                ? "Granted by macOS"
                : waitingForAccess
                  ? "System Settings is open. Turn Orbyn on under Accessibility, then come back."
                  : includeText
                    ? "Include accessibility text is on, but macOS has not allowed Accessibility. Without it captures work and carry no text."
                    : "Optional. Only for Include accessibility text, which is off."}
            >
              {accessibility ? <Pill kind="good">Granted</Pill> : waitingForAccess ? <Waiting /> : (
                <>
                  <Pill>Not granted</Pill>
                  <Button small onClick={() => void action("allow_accessibility")}>Allow Accessibility</Button>
                </>
              )}
            </Row>
          ) : null}
          <Row title="Shortcut" description={state.shortcut_registered ? `${shortcutText(chord, state.platform)} is registered.` : state.shortcut_message ?? "The shortcut is not registered."}>
            <span className="pb-keys">{keys.map((key) => <kbd key={`${key.aria}-${key.label}`}>{key.label}</kbd>)}</span>
            {!state.shortcut_registered ? <Button small onClick={() => void action("retry_shortcut")}>Try again</Button> : null}
          </Row>
          <Row
            title="Test capture"
            description={worked
              ? "Captured once. It is a draft in the composer; nothing is sent until you send it."
              : testBlocked
                ? "Available once Screen Recording is allowed."
                : "Captures the window behind Orbyn once to check that it works. The image is a draft, not sent."}
          >
            {worked ? <SnapshotWorks /> : null}
            <Button small disabled={testBlocked} onClick={() => void test()}>{worked ? "Test again" : "Test capture"}</Button>
          </Row>
          {!screenRecording ? (
            <p className="snapshot-setup-note">After you allow Screen Recording in System Settings, restart Orbyn. macOS usually needs that before a capture works.</p>
          ) : null}
        </>
      ) : (
        <>
          <Row title="Permission" description="Windows asks for no permission for Snap Shot."><Pill>Not verified</Pill></Row>
          <Row title="Capture border" description="Windows may draw a border around the window while it is captured. Orbyn does not hide it."><Pill kind="planned">Planned</Pill></Row>
          <Row title="Shortcut" description={state.shortcut_registered ? `${shortcutText(chord, state.platform)} is registered.` : state.shortcut_message ?? "The shortcut is not registered."}>
            <span className="pb-keys">{keys.map((key) => <kbd key={`${key.aria}-${key.label}`}>{key.label}</kbd>)}</span>
            {!state.shortcut_registered ? <Button small onClick={() => void action("retry_shortcut")}>Try again</Button> : null}
          </Row>
          <Row title="Accessibility text" description="Planned on Windows (UI Automation). Captures carry the image only."><Pill kind="planned">Planned</Pill></Row>
          <Row
            title="Test capture"
            description={worked ? "Captured once. It is a draft in the composer; nothing is sent until you send it." : "Captures the window behind Orbyn once to check that it works. The image is a draft, not sent."}
          >
            {worked ? <SnapshotWorks /> : null}
            <Button small onClick={() => void test()}>{worked ? "Test again" : "Test capture"}</Button>
          </Row>
        </>
      )}
      {testDraft ? (
        <SnapshotDraftChip
          name={testDraft.name}
          preview={testDraft.preview}
          source={testDraft.source}
          onRemove={() => {
            void discard(testDraft.captureId);
            URL.revokeObjectURL(testDraft.preview);
            clearTestDraft();
          }}
          onRemoveText={removeTestText}
        />
      ) : null}
    </Dialog>
  );
}
