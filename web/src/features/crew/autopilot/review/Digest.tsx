import { Button } from "../../../../ui/controls";
import { CrewFrame } from "./CrewFrame";
import { Box, EmptyState, RiskChip } from "./Parts";
import type { DigestData } from "./model";

const HEALTH = "Main is healthy · watched since 09:00";

/** What merged, what is waiting for review, what was reverted: a summary of the Review queue, never a copy of it. */
export function DigestScreen({ data, onReview }: { data: DigestData; onReview(): void }) {
  return (
    <CrewFrame
      tab="Review"
      tool="digest"
      reviewCount={7}
      mergeCount={3}
      health={HEALTH}
      crumbs={
        <>
          <span className="crew-muted">Digest</span>
          <b>{data.date}</b>
        </>
      }
    >
      {data.quiet ? (
        <EmptyState
          heading={data.quiet.heading}
          text={data.quiet.text}
          action={
            <Button small onClick={onReview}>
              {data.quiet.action}
            </Button>
          }
        />
      ) : (
        <div className="crew-pad crew-stack">
          <div className="crew-grid3">
            {data.groups.map((group) => (
              <section key={group.heading} className="crew-digest" aria-label={group.heading}>
                <span className="crew-big">{group.count}</span>
                <b>{group.heading}</b>
                <small>{group.summary}</small>
                <ul>
                  {group.rows.map((row) => (
                    <li key={row.title}>
                      <RiskChip tier={row.tier} />
                      <span>{row.title}</span>
                      <span className="crew-at">{row.at}</span>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
          {data.also ? (
            <Box heading={data.also.heading}>
              <p>{data.also.text}</p>
            </Box>
          ) : null}
        </div>
      )}
    </CrewFrame>
  );
}
