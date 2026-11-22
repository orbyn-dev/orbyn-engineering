import { Check, X } from "lucide-react";
import { useState } from "react";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { ACCESSIBILITY_TEXT_WARNING, type SnapShotSource } from "./types";
import "./snapshot.css";

const timeOf = (at: string, seconds = false): string => {
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", hour12: false, ...(seconds ? { second: "2-digit" } : {}) });
};

const sizeOf = (source: SnapShotSource): string => {
  if (source.width === undefined || source.height === undefined) return "";
  return `${source.width} × ${source.height}`;
};

const smallLine = (source: SnapShotSource) => {
  const parts = ["Not sent yet", source.window_title, sizeOf(source), timeOf(source.captured_at)].filter(Boolean);
  return parts.join(" · ");
};

/** The draft attachment chip: a capture is never sent by itself, and the person can open it and remove it. */
export function SnapshotDraftChip({
  name,
  preview,
  source,
  onRemove,
  onRemoveText,
}: {
  name: string;
  preview: string | null;
  source: SnapShotSource;
  onRemove(): void;
  /** Drops the accessible text from this capture while the image stays. */
  onRemoveText?(): void;
}) {
  const [open, setOpen] = useState(false);
  const [textGone, setTextGone] = useState(false);
  const title = `Snap Shot · ${source.app_name}`;
  const text = textGone ? undefined : source.accessible_text;
  return (
    <>
      <div className="pb-snap snapshot-chip" data-name={name}>
        <button type="button" className="snapshot-thumb" aria-label="Show Snap Shot details" onClick={() => setOpen(true)}>
          {preview ? <img src={preview} alt="" /> : <span className="snapshot-thumb-empty" aria-hidden />}
        </button>
        <div className="snapshot-chip-text">
          <b>
            <button type="button" className="snapshot-chip-title" onClick={() => setOpen(true)}>{title}</button>
            <span className="set-pill planned">Planned</span>
          </b>
          <small>{smallLine(source)}</small>
        </div>
        <button type="button" className="snapshot-chip-remove" aria-label={`Remove ${title}`} onClick={onRemove}>
          <X size={13} aria-hidden />
        </button>
      </div>
      {open ? (
        <Dialog title="Snap Shot details" onClose={() => setOpen(false)} footer={<Button onClick={() => setOpen(false)}>Done</Button>}>
          <div className="snapshot-detail-head">
            <span className="snapshot-detail-thumb">{preview ? <img src={preview} alt="" /> : null}</span>
            <span>
              <b>{source.app_name}</b>
              <small>{source.window_title}</small>
              <small>Captured {timeOf(source.captured_at, true)} · Not sent yet</small>
            </span>
          </div>
          <dl className="snapshot-detail-rows">
            <div><dt>App name</dt><dd>{source.app_name}</dd></div>
            <div><dt>Window title</dt><dd>{source.window_title}</dd></div>
            {source.app_id ? <div><dt>App id</dt><dd>{source.app_id}</dd></div> : null}
            <div><dt>Size and time</dt><dd>{[sizeOf(source), `captured ${timeOf(source.captured_at, true)}`].filter(Boolean).join(" · ")}</dd></div>
          </dl>
          {text ? (
            <div className="snapshot-accessibility">
              <div className="snapshot-accessibility-head">
                <b>Accessibility text</b>
                <Button small onClick={() => { setTextGone(true); onRemoveText?.(); }}>Remove text</Button>
              </div>
              <pre tabIndex={0}>{text}</pre>
              <small>{ACCESSIBILITY_TEXT_WARNING}</small>
            </div>
          ) : null}
        </Dialog>
      ) : null}
    </>
  );
}

/** The chip while the shell is reading the window; Cancel asks it to stop and keeps no image. */
export function SnapshotCapturingChip({ onCancel }: { onCancel(): void }) {
  return (
    <div className="pb-snap snapshot-chip snapshot-capturing" role="status">
      <span className="snapshot-thumb"><i className="spin snapshot-spin" aria-label="Working" /></span>
      <div className="snapshot-chip-text">
        <b>Snap Shot</b>
        <small>Capturing · gives up after 10 seconds</small>
      </div>
      <button type="button" className="snapshot-chip-remove" aria-label="Cancel capture" onClick={onCancel}>
        <X size={13} aria-hidden />
      </button>
    </div>
  );
}

/** The shell's success mark, for the setup dialog's test capture row. */
export const SnapshotWorks = () => <span className="snapshot-works"><Check size={13} aria-hidden />Works</span>;
