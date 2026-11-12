import { useUi } from "../state/ui";
import type { MenuItem } from "../../ui/Menu";

export const DETAIL_ROWS: { id: string; label: string }[] = [
  { id: "branch", label: "Branch" },
  { id: "changes", label: "Changes" },
  { id: "pulls", label: "Pull requests" },
  { id: "automations", label: "Automations" },
  { id: "tools", label: "Tools" },
  { id: "servers", label: "Local servers" },
  { id: "limits", label: "Limits" },
  { id: "usage", label: "Usage" },
  { id: "scripts", label: "Scripts" },
  { id: "editor", label: "Open in an outside editor" },
];

/** The rows as a checklist, for the card's menu and for Settings. */
export function detailsMenuItems(): MenuItem[] {
  const { detailsHidden, setPrefs } = useUi.getState();
  return [
    ...DETAIL_ROWS.map((row) => ({
      label: row.label,
      selected: !detailsHidden.includes(row.id),
      keepOpen: true,
      onSelect: () =>
        setPrefs({
          detailsHidden: detailsHidden.includes(row.id)
            ? detailsHidden.filter((id) => id !== row.id)
            : [...detailsHidden, row.id],
        }),
    })),
    { label: "Reset to default", separatorBefore: true, onSelect: () => setPrefs({ detailsHidden: ["usage"] }) },
  ];
}
