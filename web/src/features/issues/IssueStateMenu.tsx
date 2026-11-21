import { ChevronDown } from "lucide-react";
import { useRef, useState } from "react";
import { useUi } from "../../product/state/ui";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { issuesApi } from "./api";
import type { IssueDetail, IssueState } from "./contract";
import { IssueGlyph } from "./IssueGlyph";
import { choiceLabel, reasonOf, sameState, stateWord } from "./model";

/**
 * The State row of an issue. NOT IN THE BETA CONTRACT: docs/issues.md lists moving an issue to another state as Planned, so the real
 * service sends no `transitions` and this draws the state as text; only the mock offers the menu. A tracker that offers states shows a button that lists them in the tracker's own words and order and
 * marks the current one; one that offers none shows the state as text. Choosing a state is one write, once: while it is in
 * flight the button is off and a second choice is not a second write. The state on screen changes only when the tracker answers.
 */
export function IssueStateButton({ issue, onChanged }: { issue: IssueDetail; onChanged(): Promise<unknown> | void }) {
  const pushToast = useUi((state) => state.pushToast);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [busy, setBusy] = useState(false);
  // A second choice while the first write is in flight is not a second write.
  const pending = useRef(false);
  const choices = issue.transitions ?? [];
  if (choices.length === 0) return <span className="iss-state"><IssueGlyph state={issue.state} />{stateWord(issue.state)}</span>;
  const choose = async (next: IssueState) => {
    if (pending.current || sameState(next, issue.state)) return;
    pending.current = true;
    setBusy(true);
    try {
      await issuesApi().setState(issue, next);
      await onChanged();
    } catch (failure) {
      pushToast(`Couldn't change the state. Nothing changed. ${reasonOf(failure)}`);
    } finally {
      pending.current = false;
      setBusy(false);
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
