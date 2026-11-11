import { useState } from "react";
import { Button } from "../../../../ui/controls";
import { CrewFrame } from "./CrewFrame";
import { Act } from "./Parts";
import type { BatchData } from "./model";

/** Many Low risk changes reviewed together. A batch never holds a Medium or High change. */
export function BatchReview({ data, onLooksGood }: { data: BatchData; onLooksGood?(ticked: number): void | Promise<void> }) {
  const [off, setOff] = useState<ReadonlySet<string>>(new Set());
  const ticked = data.total - off.size;
  const toggle = (title: string) =>
    setOff((current) => {
      const next = new Set(current);
      if (!next.delete(title)) next.add(title);
      return next;
    });
  return (
    <>
      <div className="crew-bn crew-bn-plain" role="status">
        <span>
          <b>{data.lead.heading}</b> {data.lead.text}
        </span>
        <span className="crew-bn-acts">
          <Button small onClick={() => setOff(new Set(data.rows.map((row) => row.title)))}>
            Untick all
          </Button>
          <Act label={`Looks good for ${ticked}`} primary onAct={onLooksGood ? () => onLooksGood(ticked) : undefined} />
        </span>
      </div>
      <div className="crew-pad">
        <section className="crew-clist" aria-label={data.group}>
          <div className="crew-lh">
            <b>{data.group}</b>
            <span>{data.groupSummary}</span>
          </div>
          <ul>
            {data.rows.map((row) => (
              <li key={row.title} className="crew-batch-row">
                <label>
                  <input type="checkbox" checked={!off.has(row.title)} onChange={() => toggle(row.title)} />
                  <span className="crew-ck" aria-hidden />
                  <span className="crew-cmt">
                    <b>{row.title}</b>
                    <span className="crew-cmn">{row.detail}</span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
        </section>
      </div>
    </>
  );
}

export function BatchScreen({ data }: { data: BatchData }) {
  return (
    <CrewFrame tab="Review" reviewCount={7} mergeCount={3} crumbs={<b>{data.heading}</b>}>
      <BatchReview data={data} />
    </CrewFrame>
  );
}
