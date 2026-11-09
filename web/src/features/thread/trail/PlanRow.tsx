import { Check, ChevronRight, ListChecks, Loader2 } from "lucide-react";
import type { PlanItem } from "../../../product/timeline/types";
import { Collapsible } from "../../../ui/Collapsible";
import { pluralize } from "../toolCalls";
import { BlockHead, FooterText } from "./blocks/BlockParts";

/** A plan update, as a row that lists the steps. */
export function PlanRow({ item, live }: { item: PlanItem; live: boolean }) {
  if (item.steps.length === 0) return null;
  const done = (status: string | null) => status === "completed" || status === "done";
  const active = (status: string | null) => status === "in_progress" || status === "running";
  const finished = item.steps.filter((step) => done(step.status)).length;
  return (
    <section className="steps tc" aria-label="Plan">
      <div className="tg">
        <Collapsible
          className="tcw"
          persistId={`plan:${item.key}`}
          summaryClassName="tcr"
          summary={
            <>
              <span className="tic">
                <ListChecks size={14} aria-hidden />
              </span>
              <span className="lb">
                <span className="tla">Updated plan</span>
                <span className="tln">· {finished} of {item.steps.length} done</span>
              </span>
              <span className="du" />
              <span className="tcv">
                <ChevronRight size={14} aria-hidden />
              </span>
            </>
          }
        >
          <div className="cb">
            <BlockHead lead={<ListChecks size={14} />}>Plan</BlockHead>
            <ol className="td">
              {item.steps.map((step, at) => (
                <li key={at} className={`t1r ${done(step.status) ? "tdone" : active(step.status) ? "tnow" : ""}`}>
                  <span className="bx">
                    {done(step.status) ? <Check size={12} aria-hidden /> : active(step.status) && live ? <Loader2 size={12} className="spin" aria-hidden /> : null}
                  </span>
                  <span>{step.text}</span>
                  {step.status ? <span className="sr-only">({step.status.replaceAll("_", " ")})</span> : null}
                </li>
              ))}
            </ol>
            <div className="cf">
              <FooterText>{pluralize(item.steps.length, "step", "steps")}</FooterText>
            </div>
          </div>
        </Collapsible>
      </div>
    </section>
  );
}
