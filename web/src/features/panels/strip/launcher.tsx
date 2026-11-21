import { Users } from "lucide-react";
import { Pill } from "../../../ui/controls";
import type { MenuItem } from "../../../ui/Menu";

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
