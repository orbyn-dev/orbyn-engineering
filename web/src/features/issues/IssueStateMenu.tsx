import { ChevronDown } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { useUi } from "../../product/state/ui";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { issuesApi, markNotOffered, useStateRefused } from "./api";
import type { IssueDetail, IssueState } from "./contract";
import { routeAbsent } from "./edit";
import { IssueGlyph } from "./IssueGlyph";
import type { IssueWriter } from "./IssueEdit";
import { choiceLabel, reasonOf, sameState, stateWord } from "./model";

/**
 * The State row of an issue. The states it can move to are `issue.states` (docs/issues.md, "Issue writes and list controls"):
 *
 * - absent, or refused as "no such route" on first use: the service does not offer a state change. The state is text with a Planned
 *   note, and nothing is called.
 * - `[]`: the service offers it, but this tracker has no other state. The state is text, with no note.
 * - a list: a button that lists them in the tracker's own words and order and marks the current one. Choosing one is one write,
 *   `POST /api/issues/item/state` with the state's name, through the page's write guard: while it is in flight the button is off and
 *   a second choice is not a second write. The state on screen changes only when the tracker answers, with the issue it answers.
 *   A failure says nothing changed and leaves the state as it was.
 */
export function IssueStateButton({ issue, write, writing }: { issue: IssueDetail; write: IssueWriter; writing: boolean }) {
  const pushToast = useUi((state) => state.pushToast);
  const refused = useStateRefused();
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const busy = writing;
  // What the issue is in right now, not when the menu was drawn: a click on an item that is still leaving the screen after the
  // write landed is the state it is already in, and writes nothing.
  const now = useRef(issue.state);
  now.current = issue.state;
  // Set at the click, not at the next render: two quick choices in one frame are still one write.
  const sending = useRef(false);
  // The state the last write claimed: a click that lands after the write answered but before the issue re-renders
  // is the same choice, not a second write. Cleared once the issue shows a state (any answer, not only the asked one).
  const claimed = useRef<IssueState | null>(null);
  useEffect(() => { claimed.current = null; }, [issue.state]);
  const choices = issue.states ?? [];
  if (issue.states === undefined || refused) {
    return (
      <span className="iss-state">
        <IssueGlyph state={issue.state} />{stateWord(issue.state)}
        <span className="iss-quiet" title="Changing an issue's state is not available yet.">Planned</span>
      </span>
    );
  }
  if (choices.length === 0) return <span className="iss-state"><IssueGlyph state={issue.state} />{stateWord(issue.state)}</span>;
  const choose = async (next: IssueState) => {
    if (busy || sending.current || sameState(next, now.current) || (claimed.current !== null && sameState(next, claimed.current))) return;
    sending.current = true;
    claimed.current = next;
    try {
      const answered = await write(() => issuesApi().setState(issue, next));
      // A writer refusal (null) changes nothing and throws nothing: release the claim so the choice can be retried.
      if (answered === null) claimed.current = null;
    } catch (failure) {
      claimed.current = null;
      if (failure instanceof ApiFailure && routeAbsent(failure.status, failure.code, failure.message)) {
        markNotOffered("state");
        pushToast("This service does not offer state changes yet. Nothing changed.");
        return;
      }
      pushToast(`Couldn't change the state. Nothing changed. ${reasonOf(failure)}`);
    } finally {
      sending.current = false;
    }
  };
  return (
    <>
      <button
        type="button"
        className="iss-state-btn"
        aria-label={`State: ${choiceLabel(issue.state)}`}
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        disabled={busy}
        aria-busy={busy || undefined}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu(menu ? null : { x: rect.left, y: rect.bottom + 4 });
        }}
      >
        <IssueGlyph state={issue.state} />{choiceLabel(issue.state)}<ChevronDown size={12} aria-hidden />
      </button>
      <Presence>
        {menu ? (
          <Menu
            x={menu.x}
            y={menu.y}
            label="Change the state"
            minWidth={200}
            onClose={() => setMenu(null)}
            items={choices.map((choice) => ({
              label: choiceLabel(choice),
              icon: <IssueGlyph state={choice} />,
              selected: sameState(choice, issue.state),
              leadCheck: true,
              onSelect: () => void choose(choice),
            }))}
          />
        ) : null}
      </Presence>
    </>
  );
}
