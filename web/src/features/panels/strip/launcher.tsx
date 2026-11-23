import { Smartphone, Users } from "lucide-react";
import { Pill } from "../../../ui/controls";
import type { MenuItem } from "../../../ui/Menu";

/** The Devices row after Review and comments: Beta when enabled, with the service's reason when it is disabled. */
export function deviceLauncherItem(enabled: boolean, reason: string): MenuItem {
  return {
    label: "Devices",
    icon: <Smartphone size={14} aria-hidden />,
    disabled: !enabled,
    title: enabled ? "Devices" : reason.slice(0, 160),
    content: (
      <span className="menu-device-row">
        <span className="menu-device-label">Devices <Pill tone="info">Beta</Pill></span>
        {enabled ? null : <span className="menu-device-reason">{reason.slice(0, 120)}</span>}
      </span>
    ),
    onSelect: () => {},
  };
}

/**
 * Crew views, drawn in the "+" launcher after the tabs and before Open file…. The design marks it
 * Planned and it opens no submenu: that submenu lists the open thread's crew views, and nothing
 * reads a thread's crew yet, so it stays drawn and shut rather than claiming the thread has no crew.
 */
export function crewViewsItem(): MenuItem {
  return {
    label: "Crew views",
    icon: <Users size={14} aria-hidden />,
    disabled: true,
    title: "Crew views is not built yet.",
    content: (
      <span className="menu-planned-row">
        Crew views
        <Pill>Planned</Pill>
      </span>
    ),
  };
}
