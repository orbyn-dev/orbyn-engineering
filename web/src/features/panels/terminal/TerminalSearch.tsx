import { ChevronDown, ChevronUp, Search, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { IconButton } from "../../../ui/controls";
import { controlsOf, watchTerminal } from "./terminalControls";

/** Search in the scrollback of the terminal being shown: the matches are marked and the count says where you are. */
export function TerminalSearch({ keyOf, focusSignal, onClose }: { keyOf: string; focusSignal: number; onClose(): void }) {
  const [text, setText] = useState("");
  const [results, setResults] = useState<{ index: number; count: number }>({ index: -1, count: 0 });
  const field = useRef<HTMLInputElement>(null);
  // Pressing the search key again while it is open comes back to the field.
  useEffect(() => {
    field.current?.focus();
    field.current?.select();
  }, [focusSignal]);
  // Hears the results when xterm registers, which may be after the field opens.
  useEffect(() => watchTerminal(keyOf, (controls) => controls.onResults(setResults)), [keyOf]);
  useEffect(() => () => controlsOf(keyOf)?.find("", "next"), [keyOf]);
  const find = (value: string, direction: "next" | "previous") => controlsOf(keyOf)?.find(value, direction);
  const count = text === "" ? "" : results.count === 0 ? "No matches" : `${results.index + 1} of ${results.count}`;
  return (
    <div className="term-search" role="search" aria-label="Search the terminal">
      <label className="term-search-field">
        <Search size={14} aria-hidden />
        <input
          ref={field}
          aria-label="Search the terminal"
          value={text}
          autoFocus
          onChange={(event) => {
            setText(event.target.value);
            find(event.target.value, "next");
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              find(text, event.shiftKey ? "previous" : "next");
            } else if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              onClose();
            }
          }}
        />
        <span className="term-search-count" aria-live="polite">
          {count}
        </span>
      </label>
      <IconButton label="Previous match" onClick={() => find(text, "previous")}>
        <ChevronUp size={14} aria-hidden />
      </IconButton>
      <IconButton label="Next match" onClick={() => find(text, "next")}>
        <ChevronDown size={14} aria-hidden />
      </IconButton>
      <IconButton label="Close search" onClick={onClose}>
        <X size={14} aria-hidden />
      </IconButton>
    </div>
  );
}
