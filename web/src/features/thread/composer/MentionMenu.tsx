import type { ReactNode } from "react";
import { closingProps } from "../../../ui/Presence";

export interface Suggestion {
  value: string;
  detail: string | null;
  /** What goes in the prompt, when it isn't the trigger and the value. */
  insert?: string;
  /** A thread this row picks: it waits above the draft as a chip, not inline text. */
  thread?: { id: string; title: string };
}

/** The name with its matched query marked once, as the frame draws it. */
function marked(value: string, mark: string | undefined): ReactNode {
  if (!mark) return value;
  const at = value.toLowerCase().indexOf(mark.toLowerCase());
  if (at < 0) return value;
  return (
    <>
      {value.slice(0, at)}
      <b>{value.slice(at, at + mark.length)}</b>
      {value.slice(at + mark.length)}
    </>
  );
}

/** The @ and # suggestions above the composer. Leaves by fading, so it takes its rows as a snapshot. */
export function MentionMenu({
  matches,
  active,
  marker,
  mark,
  closing,
  onPick,
}: {
  matches: Suggestion[];
  active: number;
  marker: string;
  /** The typed query, marked inside each name where it appears. */
  mark?: string;
  closing?: boolean;
  onPick(suggestion: Suggestion): void;
}) {
  return (
    <ul className="mention-menu" role="listbox" aria-label="Suggestions" {...closingProps(closing)}>
      {matches.map((suggestion, at) => (
        <li key={suggestion.value} role="option" aria-selected={at === active}>
          <button type="button" tabIndex={-1} onMouseDown={(event) => event.preventDefault()} onClick={() => onPick(suggestion)}>
            <span className="mention-name">
              {marker}
              {marked(suggestion.value, mark)}
            </span>
            {suggestion.detail ? <span className="mention-detail">{suggestion.detail}</span> : null}
          </button>
        </li>
      ))}
    </ul>
  );
}
