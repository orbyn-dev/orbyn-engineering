import type { ReactNode } from "react";
import { isUnmanaged, type ManagedEngine } from "./unmanaged";

/** A pill in a picker, the frame's chip mark, or the plain warning words beside a control. */
export type UnmanagedLabelVariant = "pill" | "text" | "chip";

/**
 * The visible "Unmanaged" label rule 8 asks for, in one place. It reads the engine
 * through `isUnmanaged`, so a missing flag never draws it.
 */
export function UnmanagedLabel({
  engine,
  variant = "pill",
  icon,
}: {
  engine: ManagedEngine | null | undefined;
  variant?: UnmanagedLabelVariant;
  icon?: ReactNode;
}) {
  if (!isUnmanaged(engine)) return null;
  return (
    <span className={["unmanaged-label", `unmanaged-label-${variant}`, ...(variant === "pill" ? ["pill", "pill-warn"] : [])].join(" ")}>
      {icon}
      Unmanaged
    </span>
  );
}
