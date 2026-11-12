import { useState } from "react";
import { Button } from "../../../../ui/controls";
import { Banner } from "./Banner";
import { CrewFrame } from "./CrewFrame";
import { EmptyState } from "./Parts";
import { ReviewCard, type OpenDialog } from "./ReviewCard";
import type { QueueData, QueueEntry, QueueFilter } from "./model";

const FILTERS: { id: QueueFilter; label: string }[] = [
  { id: "to-review", label: "To review" },
  { id: "high", label: "High" },
  { id: "overdue", label: "Overdue" },
  { id: "reverted", label: "Reverted" },
];

const weight = (entry: QueueEntry) => entry.batchSize ?? 1;

/** Counts come from the entries unless the frame is a clipped sample that states them. The tab count is the same number. */
export function queueCounts(data: QueueData): Record<QueueFilter, number> {
  const live = data.entries.filter((entry) => entry.state !== "reverted");
  const sum = (list: QueueEntry[]) => list.reduce((total, entry) => total + weight(entry), 0);
  return {
    "to-review": data.counts?.["to-review"] ?? sum(live),
    high: data.counts?.high ?? sum(live.filter((entry) => entry.tier === "high")),
    overdue: data.counts?.overdue ?? sum(live.filter((entry) => entry.state === "overdue")),
    reverted: data.counts?.reverted ?? data.entries.filter((entry) => entry.state === "reverted").length,
  };
}

function visible(data: QueueData, filter: QueueFilter): QueueEntry[] {
  switch (filter) {
    case "to-review":
      return data.entries;
    case "high":
      return data.entries.filter((entry) => entry.tier === "high" && entry.state !== "reverted");
    case "overdue":
      return data.entries.filter((entry) => entry.state === "overdue");
    case "reverted":
      return data.entries.filter((entry) => entry.state === "reverted");
  }
}

/** Ranked by risk, then by the time left. Items are never invented: an empty queue says so. */
export function ReviewQueue({
  data,
  onOpen,
  onLooksGood,
}: {
  data: QueueData;
  onOpen(entry: QueueEntry, dialog?: OpenDialog): void;
  onLooksGood?(entry: QueueEntry): void | Promise<void>;
}) {
  const [filter, setFilter] = useState<QueueFilter>("to-review");
  const counts = queueCounts(data);
  const list = visible(data, filter);
  return (
    <>
      {data.banner ? (
        <Banner tone={data.banner.tone === "amber" ? "amber" : "plain"} heading={data.banner.heading} text={data.banner.text} actions={data.banner.actions.map((label) => ({ label }))} />
      ) : null}
      <div className="crew-tool crew-tool-flat">
        <div className="crew-chips" role="group" aria-label="Filter the Review queue">
          {FILTERS.map(({ id, label }) => (
            <button
              key={id}
              type="button"
              className={`crew-chip${filter === id ? " on" : ""}`}
              aria-pressed={filter === id}
              aria-disabled={counts[id] === 0 && id !== "to-review" ? true : undefined}
              disabled={counts[id] === 0 && id !== "to-review"}
              onClick={() => setFilter(id)}
            >
              {label} {counts[id]}
            </button>
          ))}
          <button type="button" className="crew-chip" aria-disabled disabled>
            Engine: any
          </button>
        </div>
        <span className="crew-health">Ranked by risk, then by the time you have left</span>
      </div>
      <div className="crew-list" role="list" aria-label="Merges to review">
        {list.map((entry) => (
          <div role="listitem" key={entry.id}>
            <ReviewCard entry={entry} onOpen={onOpen} onLooksGood={onLooksGood} />
          </div>
        ))}
      </div>
    </>
  );
}

export function ReviewQueueScreen({ data, onOpen, onDigest }: { data: QueueData; onOpen(entry: QueueEntry, dialog?: OpenDialog): void; onDigest(): void }) {
  const counts = queueCounts(data);
  const empty = data.entries.length === 0;
  return (
    <CrewFrame tab="Review" tool="review" reviewCount={counts["to-review"]} mergeCount={3} health={data.health}>
      {empty ? (
        <EmptyState
          heading="Nothing to review"
          text="Every change that reached main has been looked at. When Crew merges the next one it appears here, ranked by risk, with its proof and what wasn’t verified."
          action={
            <Button small onClick={onDigest}>
              Open the daily digest
            </Button>
          }
        />
      ) : (
        <ReviewQueue data={data} onOpen={onOpen} />
      )}
    </CrewFrame>
  );
}
