import { MessageSquare } from "lucide-react";
import { Fragment, useMemo, useState } from "react";
import { useWorkspace } from "../../../state/workspace";
import { useUi } from "../../../state/ui";
import { useComposerChips } from "../../../state/composerChipStore";
import { charactersOver, chipId, chipLabel, promptWithChips, type ReviewChip } from "../../../lib/composerContext";
import { Button } from "../../../ui/controls";
import { LINE_LIMIT, diffCommentRange, markers, numberedLines, parsePatch, splitNumbered, withoutWhitespaceChanges, type DiffLine } from "./diffModel";

function LineCell({ line, find, number, selected, onComment }: {
  line: DiffLine | null; find: string; number?: number | null; selected?: boolean; onComment?(shift: boolean): void;
}) {
  if (!line) return <div className="patch-line patch-empty"> </div>;
  const marker = markers[line.kind];
  const hit = find !== "" && line.text.toLowerCase().includes(find);
  return (
    <div className={`patch-line patch-${line.kind}`} data-hit={hit || undefined} data-selected={selected || undefined}>
      {marker ? <span className="sr-only">{marker}: </span> : null}
      {line.text || " "}
      {onComment && number ? (
        <button
          type="button"
          className="patch-line-number"
          aria-label={`Comment on line ${number}`}
          aria-pressed={Boolean(selected)}
          onClick={(event) => onComment(event.shiftKey)}
        >
          <MessageSquare size={12} aria-hidden />
          {number}
        </button>
      ) : null}
    </div>
  );
}

/**
 * A diff comment is a review chip: the composer owns it, and the patch shows it
 * under the line it ends on for as long as the chip exists. Removing either
 * removes both. `origin` ties it to this diff view, as a file comment is tied
 * to the source it was made on.
 */
export function Patch({ patch, find, path, threadId, origin }: {
  patch: string; find: string; path: string; threadId: string; origin: string;
}) {
  const ignoreSpace = useUi((state) => state.diffIgnoreWhitespace);
  const layout = useUi((state) => state.diffLayout);
  const pushToast = useUi((state) => state.pushToast);
  const cwd = useWorkspace((state) => state.threads[threadId]?.cwd);
  const chips = useComposerChips((state) => state.chips[threadId]);
  const lines = useMemo(() => {
    const parsed = parsePatch(patch);
    return ignoreSpace ? withoutWhitespaceChanges(parsed) : parsed;
  }, [patch, ignoreSpace]);
  const [all, setAll] = useState(false);
  const [selection, setSelection] = useState<{ anchor: number; end: number } | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const visible = all ? lines : lines.slice(0, LINE_LIMIT);
  const numbered = useMemo(() => numberedLines(visible), [visible]);
  if (lines.length === 0) return <p className="muted diff-binary">No text changes to show.</p>;
  const comments = (chips ?? []).filter((chip): chip is ReviewChip => chip.kind === "review" && chip.path === path
    && chip.sourceRevision === origin && (chip.sourceCwd ?? "") === (cwd ?? ""));
  const low = selection ? Math.min(selection.anchor, selection.end) : 0;
  const high = selection ? Math.max(selection.anchor, selection.end) : 0;
  const range = selection ? diffCommentRange(numbered, selection.anchor, selection.end) : null;
  const cancel = () => { setSelection(null); setText(""); setError(null); };
  const pick = (number: number, shift: boolean) => {
    setSelection((current) => ({ anchor: shift && current ? current.anchor : number, end: number }));
    setError(null);
  };
  const add = () => {
    if (!range || text.trim() === "") return;
    const chip: ReviewChip = {
      id: chipId(), kind: "review", path, ...range, comment: text.trim(), sourceRevision: origin,
      ...(cwd ? { sourceCwd: cwd } : {}),
    };
    if (chip.endLine === chip.line) delete chip.endLine;
    const current = useComposerChips.getState().chips[threadId] ?? [];
    if (charactersOver(promptWithChips(useUi.getState().drafts[threadId] ?? "", [...current, chip]))) {
      setError("This comment would make your message too long. Shorten it before adding it.");
      return;
    }
    useComposerChips.getState().add(threadId, chip);
    pushToast("Review comment added to your message");
    cancel();
  };
  const selected = (number: number | null | undefined) => selection !== null && number != null && number >= low && number <= high;
  const after = (number: number | null | undefined) => {
    if (number == null) return null;
    const here = comments.filter((chip) => (chip.endLine ?? chip.line) === number);
    const form = selection !== null && number === high;
    if (here.length === 0 && !form) return null;
    return (
      <div className="diff-annotations">
        {here.map((chip) => (
          <div key={chip.id} className="diff-comment" role="note" aria-label={`Review comment ${chipLabel(chip)}`}>
            <span className="diff-comment-head">
              <strong>You</strong>
              <span className="diff-comment-where">{chipLabel(chip)}</span>
              <span className="diff-comment-state">Review comment</span>
            </span>
            <p>{chip.comment}</p>
            <Button small tone="quiet" onClick={() => useComposerChips.getState().remove(threadId, chip.id)}>
              Remove comment
            </Button>
          </div>
        ))}
        {form ? (
          <form className="review-comment" onSubmit={(event) => { event.preventDefault(); add(); }}>
            <label htmlFor={`diff-comment-${path}`}>
              Comment on {path}:{low}{high !== low ? `–${high}` : ""}
            </label>
            <textarea
              id={`diff-comment-${path}`}
              aria-label={`Review comment on ${path} line ${low}${high !== low ? `–${high}` : ""}`}
              value={text}
              maxLength={20000}
              autoFocus
              onChange={(event) => setText(event.target.value)}
            />
            {!range ? <p role="alert">Select at most 4,000 characters of the diff for a comment.</p> : null}
            {error ? <p role="alert">{error}</p> : null}
            <div>
              <Button small tone="quiet" onClick={cancel}>Cancel</Button>
              <Button small tone="primary" type="submit" disabled={!range || text.trim() === ""}>Add to message</Button>
            </div>
          </form>
        ) : null}
      </div>
    );
  };
  return (
    <>
      <div className="patch" tabIndex={0} role="group" aria-label="Patch" data-layout={layout}>
        <div className="patch-inner">
          {layout === "split"
            ? splitNumbered(numbered).map((row, at) => (
                <Fragment key={at}>
                  <div className="patch-split">
                    <LineCell line={row.left?.line ?? null} find={find} />
                    <LineCell
                      line={row.right?.line ?? null}
                      find={find}
                      number={row.right?.number}
                      selected={selected(row.right?.number)}
                      onComment={(shift) => row.right?.number && pick(row.right.number, shift)}
                    />
                  </div>
                  {after(row.right?.number)}
                </Fragment>
              ))
            : numbered.map(({ line, number }, at) => (
                <Fragment key={at}>
                  <LineCell
                    line={line}
                    find={find}
                    number={number}
                    selected={selected(number)}
                    onComment={(shift) => number && pick(number, shift)}
                  />
                  {after(number)}
                </Fragment>
              ))}
        </div>
      </div>
      {lines.length > visible.length ? (
        <Button small tone="quiet" onClick={() => setAll(true)}>
          Show {lines.length - visible.length} more lines
        </Button>
      ) : null}
    </>
  );
}
