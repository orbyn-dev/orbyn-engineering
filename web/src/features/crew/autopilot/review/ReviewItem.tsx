import { useId, useState } from "react";
import { Button } from "../../../../ui/controls";
import { Dialog } from "../../../../ui/Dialog";
import { CrewFrame } from "./CrewFrame";
import { Act, Box, Mark, RiskChip, Status, Tag } from "./Parts";
import type { MergeItem } from "./model";

type Open = null | "comment" | "revert";

/** Backend-guarded writes. With none given every confirm is disabled and says why; nothing here merges or reverts. */
export interface ItemActions {
  looksGood?(): void | Promise<void>;
  sendBrief?(text: string): void | Promise<void>;
  revert?(reason: string, together: boolean): void | Promise<void>;
}

function Facts({ rows }: { rows: [string, string][] }) {
  return (
    <dl className="crew-kv crew-kv-wide">
      {rows.map(([label, value]) => (
        <div key={label}>
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function CommentDialog({ item, actions, onClose }: { item: MergeItem; actions: ItemActions; onClose(): void }) {
  const [text, setText] = useState(item.commentBrief.prompt);
  const field = useId();
  const send = actions.sendBrief;
  return (
    <Dialog
      title="Comment on this merge"
      className="crew-dlg"
      onClose={onClose}
      footer={
        <>
          <Button small onClick={onClose}>
            Cancel
          </Button>
          <Act
            label="Send as a fix brief"
            primary
            onAct={send ? async () => { await send(text); onClose(); } : undefined}
          />
        </>
      }
    >
      <p>Your comment becomes a fix brief. Crew opens a task, routes it, builds it and verifies it like any other, and the result comes back to your Review queue.</p>
      <label className="crew-field" htmlFor={field}>
        <span>What should change?</span>
        <textarea id={field} data-dialog-focus value={text} onChange={(event) => setText(event.target.value)} rows={3} />
      </label>
      <Box heading="The brief Crew will write">
        <p>{item.commentBrief.draft}</p>
      </Box>
    </Dialog>
  );
}

function RevertDialog({ item, actions, onClose }: { item: MergeItem; actions: ItemActions; onClose(): void }) {
  const [reason, setReason] = useState(item.revert.reason ?? "");
  const field = useId();
  const dependents = item.dependents;
  const run = actions.revert;
  const act = (together: boolean) => (run ? async () => { await run(reason, together); onClose(); } : undefined);
  if (dependents.length > 0) {
    return (
      <Dialog
        title="Another merge depends on this one"
        className="crew-dlg"
        onClose={onClose}
        footer={
          <>
            <Button small onClick={onClose} data-dialog-focus>
              Cancel
            </Button>
            <Act label="Revert this one alone" onAct={act(false)} />
            <Act label="Revert both together" primary onAct={act(true)} />
          </>
        }
      >
        <p>{dependents[0]?.note}</p>
        <Box>
          <Facts
            rows={[
              ["Reverts", item.revert.commit],
              ["Depends on it", dependents.map((entry) => entry.commit).join(", ")],
              ["Together", item.revert.together],
            ]}
          />
        </Box>
      </Dialog>
    );
  }
  return (
    <Dialog
      title="Revert this merge"
      className="crew-dlg"
      onClose={onClose}
      footer={
        <>
          <Button small onClick={onClose} data-dialog-focus>
            Cancel
          </Button>
          <Act label="Open the revert" primary onAct={act(false)} />
        </>
      }
    >
      <p>Crew opens a revert task, verifies it, and merges it through the queue. Main isn’t changed until the revert passes its checks on the combined state.</p>
      <Box>
        <Facts rows={[["Reverts", item.revert.commit], ["Later merges on top", item.revert.later], ["Time", item.revert.spent]]} />
      </Box>
      <label className="crew-field" htmlFor={field}>
        <span>
          Why? <Tag tone="lo">Optional</Tag>
        </span>
        <textarea id={field} value={reason} onChange={(event) => setReason(event.target.value)} rows={2} />
      </label>
    </Dialog>
  );
}

/** One merge opened: the sentence, the diff, the claims with their checks, what was not verified. */
export function ReviewItem({ item, initialDialog = null, actions = {}, onBack }: { item: MergeItem; initialDialog?: Open; actions?: ItemActions; onBack?(): void }) {
  const [open, setOpen] = useState<Open>(initialDialog);
  return (
    <>
      <div className="crew-rpg">
        <div className="crew-pm">
          <section className="crew-pos" aria-label={item.title}>
            <div className="crew-phd">
              <b>{item.title}</b>
              <RiskChip tier={item.tier} />
              <Status tone="ok">{item.merged}</Status>
            </div>
            <p className="crew-cl">{item.summary}</p>
            <Box>
              <Facts
                rows={[
                  ["Built by", item.builtBy],
                  ["Merged", item.mergedAt],
                  ["Time", item.spent],
                  ["Review by", item.reviewBy],
                ]}
              />
            </Box>
          </section>
          <pre className="crew-xdif" aria-label="Diff">
            {item.diff.map((line) => (
              <span key={line.text} className={line.kind === "add" ? "ad" : "dl"}>
                {line.text}
                {"\n"}
              </span>
            ))}
          </pre>
          <ul className="crew-claims" aria-label="What the change claims, and the checks">
            {item.claims.map((claim) => (
              <li key={claim.text}>
                <Mark ok={claim.ok} />
                <div>
                  <b>{claim.text}</b>
                  <span className="crew-evidence">{claim.evidence}</span>
                </div>
                <Status tone={claim.ok ? "ok" : "err"}>{claim.ok ? "Passed" : "Failed"}</Status>
              </li>
            ))}
          </ul>
        </div>
        <div className="crew-ps">
          <section className="crew-nv" aria-label="Not verified">
            <h6>Not verified</h6>
            <ul>
              {item.notVerified.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
            <span className="crew-cmn">Nothing here ran. A model’s review is advisory and wasn’t counted.</span>
          </section>
          <section className="crew-nv" aria-label="Reviewed by">
            <h6>Reviewed by</h6>
            <dl className="crew-kv">
              {item.reviewedBy.map((row) => (
                <div key={row.lane}>
                  <dt>{row.lane}</dt>
                  <dd>{row.who}</dd>
                </div>
              ))}
            </dl>
          </section>
          <div className="crew-item-acts">
            <Act label="Looks good" primary onAct={actions.looksGood} />
            <Button small onClick={() => setOpen("comment")}>
              Comment
            </Button>
            <Button small onClick={() => setOpen("revert")}>
              Revert
            </Button>
            {onBack ? (
              <Button small tone="quiet" onClick={onBack}>
                Back to the queue
              </Button>
            ) : null}
          </div>
        </div>
      </div>
      {open === "comment" ? <CommentDialog item={item} actions={actions} onClose={() => setOpen(null)} /> : null}
      {open === "revert" ? <RevertDialog item={item} actions={actions} onClose={() => setOpen(null)} /> : null}
    </>
  );
}

export function ReviewItemScreen({ item, initialDialog, reviewCount, onBack }: { item: MergeItem; initialDialog?: Open; reviewCount: number; onBack(): void }) {
  return (
    <CrewFrame
      tab="Review"
      reviewCount={reviewCount}
      mergeCount={3}
      crumbs={
        <>
          <button type="button" className="crew-link crew-muted" onClick={onBack}>
            Review queue
          </button>
          <b>{item.title}</b>
        </>
      }
    >
      <ReviewItem item={item} initialDialog={initialDialog} onBack={onBack} />
    </CrewFrame>
  );
}
