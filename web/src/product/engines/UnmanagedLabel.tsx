import type { ReactNode } from "react";
import { isUnmanaged, unmanagedReason, type ManagedEngine } from "./unmanaged";

/** A pill in a picker, the frame's chip mark, or the plain warning words beside a control. */
export type UnmanagedLabelVariant = "pill" | "text" | "chip";

/**
 * The visible "Unmanaged" label rule 8 asks for, in one place. It reads the engine
 * through `isUnmanaged`, so a missing flag never draws it. Its hover reason is the
 * engine's own sentence; `note` draws that sentence beside the word in the composer
 * context row.
 */
export function UnmanagedLabel({
  engine,
  variant = "pill",
  icon,
  note = false,
}: {
  engine: ManagedEngine | null | undefined;
  variant?: UnmanagedLabelVariant;
  icon?: ReactNode;
  note?: boolean;
}) {
  if (!isUnmanaged(engine)) return null;
  const reason = engine?.name ? unmanagedReason(engine) : null;
  return (
    <span
      className={["unmanaged-label", `unmanaged-label-${variant}`, ...(variant === "pill" ? ["pill", "pill-warn"] : [])].join(" ")}
      title={reason ?? undefined}
    >
      {icon}
      Unmanaged
      {note && reason ? <span className="unmanaged-note">{reason}</span> : null}
    </span>
  );
}
