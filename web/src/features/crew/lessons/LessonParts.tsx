import type { ReactNode } from "react";
import type { ApiFailure } from "../../../api/client";
import { NETWORK_ERROR } from "../../../api/client";
import { routeHref } from "../../../product/state/route";
import { Ic } from "../shared/parts";
import { MAX_TEXT, PERMISSIONS_NOTE, SECRET_REFUSED, leftOutWords } from "./model";
import "./lessons.css";

/** The way back above the title, as every Crew settings page draws it. */
export function BackToLessons({ repo }: { repo: string }) {
  return (
    <div style={{ display: "flex" }}>
      <a
        className="cx-t10r"
        href={routeHref({ name: "settings", section: "crew", crewPage: "lessons", repo })}
        style={{ marginLeft: "0", whiteSpace: "nowrap", display: "inline-flex", alignItems: "center", gap: "2px", color: "inherit" }}
      >
        <Ic n="left" sm />
        <span style={{ textDecoration: "underline", textUnderlineOffset: "2px" }}>Back to Lessons</span>
      </a>
    </div>
  );
}

/** The page's title: "Crew › Lesson" or "Crew › Add a lesson", with the Preview chip. */
export function LessonHead({ title }: { title: string }) {
  return (
    <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
      Crew › {title}
      <span className="cx-cwpv">Preview</span>
    </h3>
  );
}

/** The muted line every card and form ends with. */
export function PermissionsNote() {
  return (
    <p style={{ margin: "2px 0 0", display: "flex", gap: "6px", alignItems: "flex-start", color: "var(--color-muted)", fontSize: "12px", lineHeight: 1.45 }}>
      <Ic n="info" sm />
      <span>{PERMISSIONS_NOTE}</span>
    </p>
  );
}

/** The intro a form carries above its field. */
export function Intro({ children }: { children: ReactNode }) {
  return <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>{children}</p>;
}

/** The one form field a lesson's text is written in: the label, the box, and the 300-character count under it. */
export function LessonField({
  value,
  onChange,
  readOnly,
  invalid,
  autoFocus,
}: {
  value: string;
  onChange(value: string): void;
  readOnly?: boolean;
  invalid?: boolean;
  autoFocus?: boolean;
}) {
  return (
    <div className="cx-t12fl">
      <span className="cx-t12lb">Lesson</span>
      <textarea
        className={["cx-t12ip", invalid ? "cx-err" : "", readOnly ? "cx-ro" : ""].filter(Boolean).join(" ")}
        aria-label="Lesson text"
        aria-multiline="true"
        aria-readonly={readOnly || undefined}
        value={value}
        readOnly={readOnly}
        autoFocus={autoFocus}
        maxLength={MAX_TEXT}
        onChange={(event) => onChange(event.target.value)}
      />
      <span className="cx-t12lb" style={{ textAlign: "right" }}>
        {value.length} of {MAX_TEXT} characters
      </span>
    </div>
  );
}

/** The pair under a form: what is saved, and Cancel and Save. */
export function FormFoot({
  hint,
  busy,
  onCancel,
  onSave,
  saveLabel,
  saveDisabled,
}: {
  hint: string;
  busy: boolean;
  onCancel(): void;
  onSave(): void;
  saveLabel: string;
  saveDisabled?: boolean;
}) {
  return (
    <div className="cx-editfoot">
      <span className="cx-hint">{hint}</span>
      <span className="cx-acts">
        <button type="button" className="cx-ghost" disabled={busy} onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className="cx-btn" disabled={busy || saveDisabled} aria-busy={busy || undefined} onClick={onSave}>
          {saveLabel}
        </button>
      </span>
    </div>
  );
}

/** A write that failed: the service's own words, or the one sentence a service that is not there earns. */
export function WriteFailure({ title, detail, onRetry }: { title: string; detail: string; onRetry?: () => void }) {
  return (
    <div className="cx-cwbn cx-red" role="alert" style={{ margin: "0", alignItems: "flex-start" }}>
      <Ic n="alert" sm style={{ marginTop: "1px", color: "var(--color-danger)" }} />
      <span className="cx-why">
        <b>{title}</b>
        <span>{detail}</span>
      </span>
      {onRetry ? (
        <span className="cx-acts">
          <button type="button" className="cx-ghost" onClick={onRetry}>
            Try again
          </button>
        </span>
      ) : null}
    </div>
  );
}

/** What a save failure says: the service's message, or the drawn sentence for a service that is not running. */
export function saveFailureDetail(failure: ApiFailure): string {
  return failure.code === NETWORK_ERROR
    ? "Check that the Orbyn service is running. Your changes are still here, and nothing was saved."
    : failure.message;
}

/** The scan's refusal, in the contract's exact sentence: "Not saved:" then the rest of it. */
export function SecretRefused() {
  const [lead, ...rest] = SECRET_REFUSED.split(": ");
  return (
    <div className="cx-cwbn cx-red" role="alert" style={{ margin: "0", alignItems: "flex-start" }}>
      <Ic n="alert" sm style={{ marginTop: "1px", color: "var(--color-danger)" }} />
      <span>
        <b>{lead}:</b> {rest.join(": ")}
      </span>
    </div>
  );
}

/**
 * The green line after a save: the card it left, with what briefs from now on carry. A lesson the read says is not in the
 * next brief says so instead, rather than claiming briefs it will not reach.
 */
export function Saved({ kind, inNextBrief }: { kind: "added" | "edited"; inNextBrief: boolean }) {
  return (
    <div className="cx-cwbn cx-green" role="status" style={{ margin: "0" }}>
      <Ic n="ok" sm />
      <span>
        <b>Saved.</b>{" "}
        {kind === "added" ? "The lesson is on this device. " : "The lesson is changed on this device. "}
        {inNextBrief ? "Briefs that start from now on get this text." : "It is not in the next brief; the card says why."}
      </span>
    </div>
  );
}

/** A lesson that is off: it stays readable and never reaches a brief. */
export function OffBanner() {
  return (
    <div className="cx-cwbn" role="status" style={{ margin: "0" }}>
      <Ic n="info" sm />
      <span>
        <b>This lesson is off.</b> It stays here and does not go into a brief until you turn it on.
      </span>
    </div>
  );
}

/** How many briefs left it out, and what that means: the task started without it and other briefs still get it. */
export function LeftOutBanner({ count }: { count: number }) {
  return (
    <div className="cx-cwbn" role="status" style={{ margin: "0" }}>
      <Ic n="info" sm />
      <span>
        <b>Left out of {leftOutWords(count)}</b> That task started without it. The lesson stays here and goes into other briefs.
      </span>
    </div>
  );
}

/** The promotion the contract refuses while the text names a path: the service would answer 400, and the card says why. */
export function PromoteRefused({ onEdit }: { onEdit(): void }) {
  return (
    <div className="cx-cwbn cx-red" role="alert" style={{ margin: "0", alignItems: "flex-start" }}>
      <Ic n="alert" sm style={{ marginTop: "1px", color: "var(--color-danger)" }} />
      <span className="cx-why">
        <b>Can’t promote this lesson</b>
        <span>It names a path, and a lesson for all repositories may name a path class, never a path. Edit the text, then promote it.</span>
      </span>
      <span className="cx-acts">
        <button type="button" className="cx-ghost" onClick={onEdit}>
          Edit
        </button>
      </span>
    </div>
  );
}

/** One facts box: its label/value rows, at the card's own 112 px label column or the design's default 96 px. */
export function FactsBox({
  title,
  rows,
  chip,
  column = 96,
}: {
  title?: ReactNode;
  rows: readonly (readonly [string, ReactNode])[];
  chip?: ReactNode;
  column?: number;
}) {
  return (
    <div className="cx-fbx">
      {title ? (
        <h6>
          {title}
          {chip ? <span style={{ verticalAlign: "middle", marginLeft: "6px" }}>{chip}</span> : null}
        </h6>
      ) : null}
      <div className="cx-fkv" style={{ gridTemplateColumns: `${column}px minmax(0, 1fr)` }}>
        {rows.map(([label, value]) => (
          <FragmentRow key={label} label={label} value={value} />
        ))}
      </div>
    </div>
  );
}

function FragmentRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <>
      <span>{label}</span>
      <b>{value}</b>
    </>
  );
}
