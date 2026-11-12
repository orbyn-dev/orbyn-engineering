import { ArrowDown, ArrowUp, Search } from "lucide-react";
import { IconButton } from "../../../ui/controls";

/** The find bar over the diff: the text, where the current match is among all of them, and the keys that step. Esc closes it. */
export function FindBar({ value, onChange, at, total, onStep, onClose }: {
  value: string;
  onChange(value: string): void;
  at: number;
  total: number;
  onStep(step: 1 | -1): void;
  onClose(): void;
}) {
  return (
    <div
      className="x2banner x2find"
      role="search"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <Search size={14} aria-hidden />
      <input
        type="search"
        aria-label="Find in diff"
        value={value}
        autoFocus
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            onStep(event.key === "ArrowUp" || (event.key === "Enter" && event.shiftKey) ? -1 : 1);
          }
        }}
      />
      <output className="t3" aria-live="polite">
        {value === "" ? "" : total === 0 ? "No matches" : `${at + 1} of ${total}`}
      </output>
      <IconButton label="Previous match" disabled={total === 0} onClick={() => onStep(-1)}>
        <ArrowUp size={14} aria-hidden />
      </IconButton>
      <IconButton label="Next match" disabled={total === 0} onClick={() => onStep(1)}>
        <ArrowDown size={14} aria-hidden />
      </IconButton>
    </div>
  );
}
