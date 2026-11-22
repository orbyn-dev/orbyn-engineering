import { Scan } from "lucide-react";
import { shortcutHint, type SnapShotShortcut } from "./types";
import "./snapshot.css";

/**
 * The composer's Snap Shot button, beside Attach. It starts exactly the capture the shortcut starts, so a capture never
 * begins from anywhere but the person's own click or key. It is absent, not disabled, when the feature is not supported.
 */
export function SnapshotButton({
  onClick,
  capturing,
  platform,
  shortcut,
  threadId,
}: {
  onClick(): void;
  capturing: boolean;
  platform: "macos" | "windows";
  shortcut: SnapShotShortcut;
  threadId: string | null;
}) {
  const hint = shortcutHint(shortcut, platform);
  return (
    <button
      type="button"
      className={capturing ? "icon-btn snapshot-button on" : "icon-btn snapshot-button"}
      aria-label="Snap Shot"
      title={`Snap Shot · ${hint}`}
      data-thread={threadId ?? "new"}
      onClick={onClick}
    >
      <Scan size={16} aria-hidden />
      <span className="snapshot-button-hint" aria-hidden>{hint}</span>
    </button>
  );
}
