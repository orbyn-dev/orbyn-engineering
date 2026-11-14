import { Ellipsis } from "lucide-react";
import { useRef, useState } from "react";
import type { FlowCard } from "../../../generated/FlowCard";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";

const LATER = "Available when the write route ships";
const AFTER = "After the beta";

const row = (label: string, note: string): MenuItem => ({
  label,
  disabled: true,
  title: `${label}: ${note}`,
  content: (
    <>
      {label} <span className="bd-note">{note}</span>
    </>
  ),
});

/**
 * What the card's menu draws. Cancel and Re-run checks have write routes, but this view does not call them yet, so they are drawn
 * inert, "Available when the write route ships", and only for a task they could apply to (the card's advisory flags). The rest
 * are target design: After the beta, and inert. Nothing in it can be chosen.
 */
export function cardItems(card: Pick<FlowCard, "can_cancel" | "can_rerun" | "state">): MenuItem[] {
  return [
    ...(card.can_cancel ? [row("Cancel", LATER)] : []),
    ...(card.can_rerun ? [row("Re-run checks", LATER)] : []),
    row("Reassign engine", AFTER),
    row("Message into a lane", AFTER),
    row("Add reviewer lane", AFTER),
    ...(card.state === "needs_person" ? [row("Answer a worker’s question", AFTER)] : []),
    row("Propose a check", AFTER),
  ];
}

export function CardMenu({ card }: { card: FlowCard }) {
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
      <Presence>{at ? <Menu x={at.x} y={at.y} label="Task actions" items={cardItems(card)} onClose={() => setAt(null)} /> : null}</Presence>
    </>
  );
}
