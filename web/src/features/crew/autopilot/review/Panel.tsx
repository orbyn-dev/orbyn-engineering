import { MoreHorizontal } from "lucide-react";
import { Button } from "../../../../ui/controls";
import { Act, Box, RiskChip, Status } from "./Parts";
import { mergeQueue } from "./sampleMerge";
import { highKey, mediumRedirect } from "./sampleQueue";
import type { QueueEntry } from "./model";

const VIEWS = ["Flow", "Review", "Map", "Board", "Lanes", "Inbox 4"] as const;
type PanelView = "review" | "merge";

const brief = (entry: QueueEntry) => {
  const proof = entry.proof.filter((phrase) => phrase.startsWith("Checks") || phrase.startsWith("Not verified"));
  return proof.join(" · ");
};

/** The Crew tab in the side panel, 320 px wide: each merge is a card with the tier, the time left, the proof in one line, and the actions. */
export function CrewPanel({ view, onView, onOpenInCrew }: { view: PanelView; onView(view: PanelView): void; onOpenInCrew(): void }) {
  const cards: { entry: QueueEntry; when: string }[] = [
    { entry: highKey, when: "top of queue" },
    { entry: mediumRedirect, when: "22 h left" },
  ];
  return (
    <section className="crew-panel" aria-label="Crew">
      <p className="crew-notice" role="note">
        <strong>Sample data.</strong> Crew is not connected to this app yet.
      </p>
      <header className="crew-panel-head">
        <b>Crew</b>
        <span className="crew-pv">Preview</span>
        <Button small onClick={onOpenInCrew}>
          Open in Crew
        </Button>
      </header>
      <div className="crew-seg" role="tablist" aria-label="Crew views">
        {VIEWS.map((name) => (
          <button key={name} type="button" role="tab" aria-selected={name === "Review"} aria-disabled={name === "Review" ? undefined : true} tabIndex={name === "Review" ? 0 : -1} className={name === "Review" ? "on" : undefined}>
            {name}
          </button>
        ))}
        <button type="button" className="crew-ovf" aria-label="More Crew views" aria-disabled tabIndex={-1}>
          <MoreHorizontal size={14} aria-hidden />
        </button>
      </div>
      <div className="crew-panel-body">
        <div className="crew-seg" role="tablist" aria-label="Autopilot views">
          <button type="button" role="tab" aria-selected={view === "review"} className={view === "review" ? "on" : undefined} onClick={() => onView("review")}>
            Review
          </button>
          <button type="button" role="tab" aria-selected={view === "merge"} className={view === "merge" ? "on" : undefined} onClick={() => onView("merge")}>
            Merge queue
          </button>
          <button type="button" role="tab" aria-selected={false} aria-disabled tabIndex={-1}>
            Digest
          </button>
        </div>
        {view === "review"
          ? cards.map(({ entry, when }) => (
              <Box key={entry.id}>
                <div className="crew-panel-line">
                  <RiskChip tier={entry.tier} />
                  <span>{when}</span>
                </div>
                <b>{entry.title}</b>
                <span className="crew-cmn">{brief(entry)}</span>
                <div className="crew-panel-acts">
                  <Act label="Looks good" primary />
                  <Button small onClick={onOpenInCrew}>
                    Comment
                  </Button>
                  <Button small onClick={onOpenInCrew}>
                    Revert
                  </Button>
                </div>
              </Box>
            ))
          : mergeQueue.rows.map((row) => (
              <Box key={row.n}>
                <div className="crew-panel-line">
                  <span className="crew-n">{row.n}</span>
                  <Status tone={row.state === "merged" ? "ok" : row.state === "running" ? "info" : "neutral"} working={row.state === "running"}>
                    {row.status}
                  </Status>
                  <RiskChip tier={row.tier} />
                </div>
                <b>{row.title}</b>
              </Box>
            ))}
      </div>
    </section>
  );
}
