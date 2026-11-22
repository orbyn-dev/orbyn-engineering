import { useEffect, useState, type ReactNode } from "react";
import { Paperclip, Scan, Send } from "lucide-react";
import { Button } from "../../ui/controls";
import { Toasts } from "../../product/ui/Toasts";
import { useUi } from "../../product/state/ui";
import { resetSnapshotRuntimeForTests, setSnapshotPreviewQuiet, useSnapshot } from "./runtime";
import { installedSnapshotMock, snapshotTransportReady } from "./shell";
import { SnapshotRuntime } from "./SnapshotRuntime";
import { SnapshotSettingsSection } from "./SnapshotSettingsSection";
import { SnapshotCapturingChip, SnapshotDraftChip } from "./SnapshotChip";
import { useSnapshotDraftFlow } from "./useSnapshotDraftFlow";
import { isSnapshotFrame, SNAPSHOT_FRAMES, withoutAccessibleText, type SnapShotDraft } from "./types";
import "./preview.css";

const TOAST_FRAMES = new Set([
  "snapshot-captured",
  "snapshot-no-thread",
  "snapshot-excluded",
  "snapshot-excluded-narrow",
  "snapshot-failed",
  "snapshot-failed-no-window",
  "snapshot-failed-too-large",
  "snapshot-failed-timeout",
  "snapshot-failed-error",
  "snapshot-failed-narrow",
]);

const CHIP_FRAMES = new Set(["snapshot-draft", "snapshot-details", "snapshot-captured", "snapshot-no-thread", "snapshot-narrow"]);
const CAPTURE_FRAMES = new Set([
  ...CHIP_FRAMES,
  "snapshot-capturing",
  "snapshot-excluded",
  "snapshot-excluded-narrow",
  "snapshot-failed",
  "snapshot-failed-no-window",
  "snapshot-failed-too-large",
  "snapshot-failed-timeout",
  "snapshot-failed-error",
  "snapshot-failed-narrow",
]);

function frameOf(hash: string): string {
  const query = hash.split("?")[1] ?? "";
  const frame = new URLSearchParams(query).get("frame") ?? "";
  return isSnapshotFrame(frame) ? frame : SNAPSHOT_FRAMES[0];
}

const PreviewRow = ({ title, description, children }: { title: string; description: string; children?: ReactNode }) => (
  <div className="setting-line">
    <div className="setting-line-text"><span className="setting-line-title">{title}</span><span className="setting-line-desc">{description}</span></div>
    <div className="setting-line-control">{children}</div>
  </div>
);

function PermissionRows({ frame }: { frame: string }) {
  const state = useSnapshot((store) => store.state);
  const action = useSnapshot((store) => store.setupAction);
  if (!state || state.platform !== "macos" || frame === "snapshot-settings-linux") return null;
  const permission = (title: string, description: string, granted: boolean, allow: () => void) => (
    <PreviewRow key={title} title={title} description={granted ? "Granted by macOS" : description}>
      {granted ? <span className="set-pill good">Granted</span> : <><span className="set-pill">Not granted</span><Button small onClick={allow}>Open System Settings</Button></>}
    </PreviewRow>
  );
  return (
    <section className="setting-group">
      <div className="setting-list">
        {permission("Accessibility", "Not granted.", state.permissions.accessibility, () => void action("allow_accessibility"))}
        {permission("Screen Recording", "Not granted. Needed to capture the screen for a thread.", state.permissions.screen_recording, () => void action("allow_screen_recording"))}
      </div>
    </section>
  );
}

function SettingsFrame({ frame }: { frame: string }) {
  const linux = frame === "snapshot-settings-linux";
  return (
    <main className="snapshot-preview snapshot-preview-settings">
      <div className="snapshot-preview-page">
        <h1>Settings</h1>
        <p className="settings-scope">Desktop app · applies to this computer</p>
        {linux ? (
          <section className="setting-group"><div className="setting-list">
            <PreviewRow title="Command line tool" description="Not installed. Installs orbyn into ~/.local/bin."><Button small>Install</Button></PreviewRow>
          </div></section>
        ) : (
          <>
            <PermissionRows frame={frame} />
            <SnapshotSettingsSection />
          </>
        )}
      </div>
    </main>
  );
}

function ComposerFrame({ frame }: { frame: string }) {
  const [chip, setChip] = useState<SnapShotDraft | null>(null);
  const noThread = frame === "snapshot-no-thread";
  const flow = useSnapshotDraftFlow({
    kind: noThread ? "draft" : "thread",
    threadId: noThread ? null : "preview-thread",
    label: "Fix login redirect",
    receive: async (draft) => {
      setChip(draft);
    },
  });

  useEffect(() => {
    if (!CAPTURE_FRAMES.has(frame)) return;
    const timer = setTimeout(() => void useSnapshot.getState().captureFromPerson(), 120);
    return () => clearTimeout(timer);
  }, [frame]);

  return (
    <main className="snapshot-preview snapshot-preview-thread">
      <div className="snapshot-preview-thread-body" />
      <div className="composer preview-composer">
        <div className="composer-inner">
          <div className="composer-box">
            {flow.capturing ? <SnapshotCapturingChip onCancel={flow.cancel} /> : null}
            {chip ? (
              <ul className="attachment-strip" aria-label="Attachments">
                <li className="attachment-snapshot">
                  <SnapshotDraftChip
                    name={chip.name}
                    preview={chip.preview}
                    source={chip.source}
                    onRemove={() => {
                      void useSnapshot.getState().discard(chip.captureId);
                      setChip(null);
                    }}
                    onRemoveText={() => setChip((current) => current ? { ...current, source: withoutAccessibleText(current.source) } : current)}
                  />
                </li>
              </ul>
            ) : null}
            <textarea rows={1} aria-label="Message" placeholder="How can Orbyn help you today?" readOnly />
            <div className="composer-bar">
              <span className="composer-left">
                <button type="button" className="icon-btn" aria-label="Attach files"><Paperclip size={16} aria-hidden /></button>
                {flow.button ?? <button type="button" className="icon-btn" aria-label="Snap Shot"><Scan size={16} aria-hidden /></button>}
              </span>
              <span className="composer-spacer" />
              <span className="composer-control">Opus 5.5</span>
              <span className="composer-control">High</span>
              <span className="composer-control">Supervised</span>
              <button type="button" className="send-btn" aria-label="Send"><Send size={15} aria-hidden /></button>
            </div>
          </div>
        </div>
      </div>
    </main>
  );
}

/** The dev-only page for the snapshot-* frames: `#/preview/snapshot?frame=<id>`. It uses the real section, chip and dialog against the mock. */
export default function SnapshotPreview() {
  const [frame] = useState(() => frameOf(window.location.hash));
  const [ready, setReady] = useState(false);
  const settingsFrame = frame.includes("settings");

  useEffect(() => {
    document.title = `Snap Shot preview · ${frame}`;
    let live = true;
    setReady(false);
    void (async () => {
      await snapshotTransportReady();
      if (!live) return;
      const mock = installedSnapshotMock();
      mock?.setFrame(frame);
      resetSnapshotRuntimeForTests();
      setSnapshotPreviewQuiet(!TOAST_FRAMES.has(frame));
      useUi.getState().setPrefs({
        snapshotEnabled: !frame.endsWith("-off") && frame !== "snapshot-settings-linux" && !frame.includes("off-narrow"),
        snapshotIncludeAccessibility: frame === "snapshot-settings-accessibility" || frame === "snapshot-setup-accessibility",
        snapshotExcludedApps: frame.includes("excluded") ? ["com.example.vault", "com.example.notes-secure"] : [],
      });
      await useSnapshot.getState().start();
      if (frame.startsWith("snapshot-setup")) {
        useSnapshot.getState().openSetup(frame === "snapshot-setup-accessibility" ? "accessibility" : "all");
      } else {
        useSnapshot.getState().closeSetup();
      }
      if (live) setReady(true);
    })();
    return () => {
      live = false;
    };
  }, [frame]);

  if (!ready) return null;
  return (
    <>
      <SnapshotRuntime />
      <Toasts />
      {settingsFrame ? <SettingsFrame frame={frame} /> : <ComposerFrame frame={frame} />}
    </>
  );
}
