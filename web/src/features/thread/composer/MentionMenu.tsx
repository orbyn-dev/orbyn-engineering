import { closingProps } from "../../../ui/Presence";

export interface Suggestion {
  value: string;
  detail: string | null;
  /** What goes in the prompt, when it isn't the trigger and the value. */
  insert?: string;
}

/** The @ and # suggestions above the composer. Leaves by fading, so it takes its rows as a snapshot. */
export function MentionMenu({
  matches,
  active,
  marker,
  closing,
  onPick,
}: {
  matches: Suggestion[];
  active: number;
  marker: string;
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
              {suggestion.value}
            </span>
            {suggestion.detail ? <span className="mention-detail">{suggestion.detail}</span> : null}
          </button>
        </li>
      ))}
    </ul>
  );
}
