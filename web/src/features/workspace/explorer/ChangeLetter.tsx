import type { ExplorerChange } from "../../../generated/ExplorerChange";
import { CHANGE } from "./model";

/** The class a change letter takes: its colour step, or the neutral bold mark of a conflict. */
const LETTER_CLASS: Record<ExplorerChange, string> = {
  modified: "m",
  added: "a",
  renamed: "r",
  untracked: "u",
  deleted: "d",
  conflicted: "xg-bang",
};

/** One change letter, drawn the same in the explorer's rows and the Changes panel's file lists. */
export function ChangeLetter({ change }: { change: ExplorerChange }) {
  const entry = CHANGE[change];
  if (!entry) return null;
  return (
    <span className={`xm ${LETTER_CLASS[change]}`} title={entry.label} aria-label={entry.label}>
      {entry.letter}
    </span>
  );
}

/** A name that gives way in the middle, so the ending that tells two files apart stays in view. */
export function MiddleName({ text, middle }: { text: string; middle: boolean }) {
  if (!middle || text.length <= 10) return <span className="nm">{text}</span>;
  return (
    <span className="nm nm-mid" role="text" aria-label={text}>
      <span className="nm-head" aria-hidden>{text.slice(0, -7)}</span>
      <span className="nm-tail" aria-hidden>{text.slice(-7)}</span>
    </span>
  );
}
