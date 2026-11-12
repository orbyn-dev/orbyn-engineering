import { Button } from "../../../../ui/controls";
import { Act, RiskChip, Tag } from "./Parts";
import type { QueueEntry } from "./model";

export type OpenDialog = "comment" | "revert";

/**
 * One merge in the Review queue. The title opens the item; Comment and Revert open the same item with
 * their dialog, and only that dialog's confirm can act. "Looks good" is a write and stays guarded.
 */
export function ReviewCard({
  entry,
  onOpen,
  onLooksGood,
}: {
  entry: QueueEntry;
  onOpen(entry: QueueEntry, dialog?: OpenDialog): void;
  onLooksGood?(entry: QueueEntry): void | Promise<void>;
}) {
  const classes = ["crew-apq", entry.top ? "crew-apq-top" : "", entry.state === "overdue" ? "crew-apq-od" : "", entry.state === "reverted" ? "crew-apq-rv" : ""];
  const batch = entry.batchSize ? entry.batchSize : 0;
  return (
    <article className={classes.filter(Boolean).join(" ")} aria-label={`${entry.tier} risk: ${entry.title}`}>
      <div className="crew-apq-rt">
        <RiskChip tier={entry.tier} />
      </div>
      <div className="crew-apq-tt">
        <button type="button" className="crew-link" onClick={() => onOpen(entry)}>
          {entry.title}
        </button>
        <div className="crew-apq-m">
          {entry.meta}
          {entry.marker ? <span className="crew-mk">{entry.marker}</span> : null}
        </div>
        <div className="crew-apq-pf">
          {entry.proof.map((phrase) => (
            <span key={phrase}>{phrase}</span>
          ))}
        </div>
      </div>
      <div className="crew-apq-ac">
        {entry.state === "reverted" ? (
          <>
            <Act label="Open the revert" primary />
            <Button small onClick={() => onOpen(entry, "comment")}>
              Comment
            </Button>
          </>
        ) : batch ? (
          <>
            <Act label={`Looks good for all ${batch}`} primary onAct={onLooksGood ? () => onLooksGood(entry) : undefined} />
            <Button small onClick={() => onOpen(entry)}>
              {`Open the ${batch}`}
            </Button>
          </>
        ) : (
          <>
            <Act label="Looks good" primary onAct={onLooksGood ? () => onLooksGood(entry) : undefined} />
            <Button small onClick={() => onOpen(entry, "comment")}>
              Comment
            </Button>
            <Button small onClick={() => onOpen(entry, "revert")}>
              Revert
            </Button>
          </>
        )}
      </div>
      <div className="crew-apq-rb">
        {entry.overdueTag ? <Tag tone="hi">{entry.overdueTag}</Tag> : null}
        {entry.footer}
      </div>
    </article>
  );
}
