import type { Checkpoint, CommitInfo } from "../../../api/types";
import { pickerTime } from "../../../lib/format";
import type { MenuItem } from "../../../ui/Menu";

/** What one turn changed, read from its own diff. */
export interface TurnTotals {
  added: number;
  removed: number;
  files: number;
}

const count = (value: number) => value.toLocaleString("en-US");

const clock = (iso: string) => {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? "" : `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
};

/** The turn picker's rows, newest first: its number, when it ended, its totals and how many files it changed. */
export function turnItems(turns: readonly Checkpoint[], totals: readonly (TurnTotals | null)[], current: string | null, onPick: (runId: string) => void): MenuItem[] {
  return turns
    .map((turn, at) => {
      const own = totals[at];
      const caption = [
        clock(turn.created_at),
        own ? `+${count(own.added)} −${count(own.removed)}` : "",
        own ? (own.files === 1 ? "1 file" : `${count(own.files)} files`) : "",
      ].filter(Boolean).join(" · ");
      return {
        label: `Turn ${at + 1}`,
        selected: turn.run_id === current,
        leadCheck: true,
        ...(caption ? { shortcut: caption } : {}),
        onSelect: () => onPick(turn.run_id),
      } satisfies MenuItem;
    })
    .reverse();
}

/** The two letters that stand for an author. */
export const initialsOf = (author: string) =>
  author.trim().split(/\s+/).filter(Boolean).slice(0, 2).map((part) => part[0]?.toUpperCase() ?? "").join("");

/** The commit picker's rows, newest first as the service lists them: short hash, subject, author and time. */
export function commitItems(commits: readonly CommitInfo[], current: string | null, onPick: (sha: string) => void): MenuItem[] {
  return commits.map((commit) => {
    const who = initialsOf(commit.author);
    const when = pickerTime(commit.date);
    return {
      label: `${commit.sha.slice(0, 7)} ${commit.subject}`,
      selected: commit.sha === current,
      leadCheck: true,
      content: (
        <>
          <span className="pk-hash">{commit.sha.slice(0, 7)}</span>
          <span className="pk-subject">{commit.subject}</span>
          {who ? <span className="pk-who" aria-hidden>{who}</span> : null}
          {when ? <span className="pk-when">{when}</span> : null}
        </>
      ),
      onSelect: () => onPick(commit.sha),
    } satisfies MenuItem;
  });
}
