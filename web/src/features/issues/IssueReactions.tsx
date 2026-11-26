import { Plus } from "lucide-react";
import { useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { canSendPersonKey } from "../../api/personKey";
import { useUi } from "../../product/state/ui";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { issuesApi, markNotOffered, useReactRefused } from "./api";
import type { IssueDetail, IssueReaction } from "./contract";
import { routeAbsent } from "./edit";
import type { IssueWriter } from "./IssueEdit";
import { capText, reasonOf } from "./model";

/** The eight the react route takes, in issue-write-reactions' order and words. Azure DevOps has six and refuses the other two. */
const REACTIONS: { value: IssueReaction; label: string }[] = [
  { value: "thumbs_up", label: "Thumbs up" },
  { value: "thumbs_down", label: "Thumbs down" },
  { value: "laugh", label: "Laugh" },
  { value: "hooray", label: "Hooray" },
  { value: "confused", label: "Confused" },
  { value: "heart", label: "Heart" },
  { value: "rocket", label: "Rocket" },
  { value: "eyes", label: "Eyes" },
];

const KNOWN: Record<string, string> = {
  thumbsup: "Thumbs up", thumbsdown: "Thumbs down", laugh: "Laugh", laughing: "Laugh",
  hooray: "Hooray", confused: "Confused", heart: "Heart", rocket: "Rocket", eyes: "Eyes",
};

/** The tracker's own word for a reaction (`THUMBS_UP`) mapped to the name every tracker's word means; an unknown one keeps its word. */
function reactionWord(content: string): string {
  return KNOWN[content.toLowerCase().replace(/[^a-z]/g, "")] ?? capText(content, 40);
}

/**
 * The reaction bar under the issue's description (issue-write-reactions): the counts the tracker reported, and the one control
 * that adds another. Add only: the contract's counts carry no "did this person react", so the page cannot show which one to take
 * back. A plain detail read carries no counts at all, so the bar is only as fresh as the last write.
 *
 * One choice is one `POST /api/issues/item/react` through the page's write guard; the page then shows the issue it answers. A
 * refusal says so and changes nothing. The route is Person-class: a browser tab has no shell to send the key, so it hides the
 * control rather than offer one that cannot work. A first "no such route" hides the bar for the life of the client.
 */
export function ReactionBar({ issue, write, writing }: { issue: IssueDetail; write: IssueWriter; writing: boolean }) {
  const refused = useReactRefused();
  const pushToast = useUi((state) => state.pushToast);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const sending = useRef(false);
  if (!canSendPersonKey() || refused) return null;
  const counts = issue.reactions ?? [];
  const add = async (reaction: IssueReaction) => {
    if (writing || sending.current) return;
    sending.current = true;
    try {
      await write(() => issuesApi().react(issue, reaction, true));
    } catch (failure) {
      if (failure instanceof ApiFailure && routeAbsent(failure.status, failure.code, failure.message)) {
        markNotOffered("react");
        return;
      }
      pushToast(`Couldn't add the reaction. Nothing changed. ${reasonOf(failure)}`);
    } finally {
      sending.current = false;
    }
  };
  return (
    <div className="iss-react" role="group" aria-label="Reactions">
      {counts.map((one, at) => (
        <span key={`${at}:${one.content}`} className="iss-react-chip">{reactionWord(one.content)}<b>{one.count}</b></span>
      ))}
      <button
        type="button"
        className="iss-react-add"
        aria-haspopup="menu"
        aria-expanded={menu !== null}
        disabled={writing}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setMenu(menu ? null : { x: rect.left, y: rect.bottom + 4 });
        }}
      >
        <Plus size={14} aria-hidden />Add reaction
      </button>
      <Presence>
        {menu ? (
          <Menu
            x={menu.x}
            y={menu.y}
            label="Add reaction"
            minWidth={208}
            onClose={() => setMenu(null)}
            items={REACTIONS.map(({ value, label }) => ({ label, onSelect: () => void add(value) }))}
          />
        ) : null}
      </Presence>
    </div>
  );
}
