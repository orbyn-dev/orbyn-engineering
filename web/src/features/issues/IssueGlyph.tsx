import { Check, Slash } from "lucide-react";
import type { IssueState } from "./contract";

/** An open ring, or a closed ring with a tick (a slash when it was closed as not planned): the state as a shape, so it never rests on colour alone. */
export function IssueGlyph({ state, size = 14 }: { state: IssueState; size?: number }) {
  const closed = state.category === "closed";
  return (
    <span className="iss-glyph" data-state={state.category} style={{ width: size, height: size }} aria-hidden>
      {closed ? (state.close_reason === "not_planned" ? <Slash size={size - 6} strokeWidth={3} /> : <Check size={size - 6} strokeWidth={3} />) : null}
    </span>
  );
}
