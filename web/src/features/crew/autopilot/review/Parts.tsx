import { Check, Diff, FileCode2, KeyRound, RotateCcw, Shield, ShieldCheck, X } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "../../../../ui/controls";
import { EFFECT_LABEL, TIER_LABEL, type Effect, type ReasonIcon, type Tier } from "./model";

export const UNAVAILABLE_REASON = "Crew is not connected to this app yet";

/** Spread onto every control that would change a repository or the crew: disabled, with the reason for everyone. */
export const unavailable = {
  disabled: true,
  "aria-disabled": true,
  "aria-description": UNAVAILABLE_REASON,
  title: UNAVAILABLE_REASON,
} as const;

/**
 * An action that writes. It is enabled only when the caller passes the backend-guarded handler;
 * with none it is disabled and says why. No screen here can merge or revert on its own.
 */
export function Act({
  label,
  primary,
  onAct,
  busy,
}: {
  label: string;
  primary?: boolean;
  onAct?: () => void | Promise<void>;
  busy?: boolean;
}) {
  return onAct ? (
    <Button small tone={primary ? "primary" : "secondary"} onClick={() => void onAct()} busy={busy}>
      {label}
    </Button>
  ) : (
    <Button small tone={primary ? "primary" : "secondary"} {...unavailable}>
      {label}
    </Button>
  );
}

/** The tier chip: always one click from its reasons wherever it appears. */
export function RiskChip({ tier, onClick }: { tier: Tier; onClick?: () => void }) {
  const className = `crew-risk crew-risk-${tier}`;
  return onClick ? (
    <button type="button" className={className} onClick={onClick} aria-haspopup="dialog">
      {TIER_LABEL[tier]}
    </button>
  ) : (
    <span className={className}>{TIER_LABEL[tier]}</span>
  );
}

export type StatusTone = "ok" | "info" | "warn" | "err" | "neutral";

/** A status with a dot, or a spinner while work runs (blue only then). */
export function Status({ tone, working, children }: { tone: StatusTone; working?: boolean; children: ReactNode }) {
  return (
    <span className={`crew-st crew-st-${tone}`}>
      {working ? <i className="crew-spin" role="img" aria-label="Working" /> : <i className="crew-dot" aria-hidden />}
      {children}
    </span>
  );
}

const TAG_CLASS = { hi: "crew-tag crew-tag-hi", lo: "crew-tag crew-tag-lo" } as const;
const BOX_CLASS = { amber: "crew-box crew-box-amber", red: "crew-box crew-box-red" } as const;

export function Tag({ children, tone }: { children: ReactNode; tone?: "hi" | "lo" }) {
  return <span className={tone ? TAG_CLASS[tone] : "crew-tag"}>{children}</span>;
}

const REASON_ICON: Record<ReasonIcon, ReactNode> = {
  paths: <FileCode2 size={14} aria-hidden />,
  size: <Diff size={14} aria-hidden />,
  coverage: <ShieldCheck size={14} aria-hidden />,
  "revert-rate": <RotateCcw size={14} aria-hidden />,
  "area-mode": <Shield size={14} aria-hidden />,
  security: <KeyRound size={14} aria-hidden />,
};
export const reasonIcon = (icon: ReasonIcon) => REASON_ICON[icon];

export function EffectTag({ effect }: { effect: Effect }) {
  return <Tag tone={effect === "raises" ? undefined : "lo"}>{EFFECT_LABEL[effect]}</Tag>;
}

/** A pass or fail mark that does not rely on colour alone. */
export function Mark({ ok }: { ok: boolean }) {
  return (
    <span className={`crew-mark ${ok ? "crew-mark-ok" : "crew-mark-bad"}`} role="img" aria-label={ok ? "Passed" : "Failed"}>
      {ok ? <Check size={12} aria-hidden /> : <X size={12} aria-hidden />}
    </span>
  );
}

/** A grey box with a heading and one paragraph or a list of rows. */
export function Box({
  heading,
  tone,
  children,
}: {
  heading?: string;
  tone?: "amber" | "red";
  children?: ReactNode;
}) {
  return (
    <div className={tone ? BOX_CLASS[tone] : "crew-box"}>
      {heading ? <h6>{heading}</h6> : null}
      {children}
    </div>
  );
}

export function EmptyState({ heading, text, action }: { heading: string; text: string; action?: ReactNode }) {
  return (
    <div className="crew-state" role="status">
      <b>{heading}</b>
      <p>{text}</p>
      {action}
    </div>
  );
}
