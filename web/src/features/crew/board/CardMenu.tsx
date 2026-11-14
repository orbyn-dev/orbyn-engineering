import { Ellipsis } from "lucide-react";
import { useRef, useState } from "react";
import type { FlowCard } from "../../../generated/FlowCard";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import type { Can } from "../writes/TaskWrite";

const AFTER = "After the beta";

const inert = (label: string, note: string): MenuItem => ({
  label,
  disabled: true,
  title: `${label}: ${note}`,
  content: (
    <>
      {label} <span className="bd-note">{note}</span>
    </>
  ),
});

/** Cancel and Re-run checks, as the card's flags and the write state allow them. `ask` opens the confirm; nothing is sent by it. */
function write(label: string, can: Can, ask: () => void): MenuItem {
  if (can.enabled) return { label, onSelect: ask };
  return can.why ? inert(label, can.why) : inert(label, "Not available for this task");
}

/**
 * What the card's menu draws. Cancel and Re-run checks are drawn only for a task they could apply to (the card's advisory flags: the
 * service decides) and, chosen, open a confirm first. The rest are target design: After the beta, and inert.
 */
export function cardItems(
  card: Pick<FlowCard, "can_cancel" | "can_rerun" | "state">,
  actions: { can(kind: "cancel" | "rerun"): Can; ask(kind: "cancel" | "rerun"): void },
): MenuItem[] {
  return [
    ...(card.can_cancel ? [write("Cancel", actions.can("cancel"), () => actions.ask("cancel"))] : []),
    ...(card.can_rerun ? [write("Re-run checks", actions.can("rerun"), () => actions.ask("rerun"))] : []),
    inert("Reassign engine", AFTER),
    inert("Message into a lane", AFTER),
    inert("Add reviewer lane", AFTER),
    ...(card.state === "needs_person" ? [inert("Answer a worker’s question", AFTER)] : []),
    inert("Propose a check", AFTER),
  ];
}

export function CardMenu({ card, actions }: { card: FlowCard; actions: Parameters<typeof cardItems>[1] }) {
  const button = useRef<HTMLButtonElement>(null);
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <>
      <button
        ref={button}
        type="button"
        className="bd-more"
        aria-label={`Actions for ${card.name || "this task"}`}
        aria-haspopup="menu"
        aria-expanded={at !== null}
        onClick={() => {
          const box = button.current?.getBoundingClientRect();
          setAt(at ? null : { x: box?.left ?? 0, y: (box?.bottom ?? 0) + 4 });
        }}
      >
        <Ellipsis size={14} aria-hidden />
      </button>
      <Presence>{at ? <Menu x={at.x} y={at.y} label="Task actions" items={cardItems(card, actions)} onClose={() => setAt(null)} /> : null}</Presence>
    </>
  );
}
