import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Act, Box, EffectTag, RiskChip, reasonIcon, Tag } from "./Parts";
import { Banner } from "./Banner";
import { CrewFrame } from "./CrewFrame";
import { TIER_LABEL, type RiskReport } from "./model";
import { popoverRows } from "./sampleRisk";
import * as risk from "./sampleRisk";
import { board } from "./sampleMerge";

/** One task's tier and every reason behind it. */
export function RiskCard({ report, heading }: { report: RiskReport; heading?: string }) {
  return (
    <section className="crew-card" aria-label={`${heading ?? report.title}: ${TIER_LABEL[report.tier]} risk`}>
      <h6>
        <RiskChip tier={report.tier} />
        <span>{heading ?? report.title}</span>
      </h6>
      <ul className="crew-reasons">
        {report.reasons.map((reason) => (
          <li key={reason.label}>
            {reasonIcon(reason.icon)}
            <b>{reason.label}</b>
            <span>{reason.value}</span>
            <EffectTag effect={reason.effect} />
          </li>
        ))}
      </ul>
      {report.outcome ? (
        <Box heading={report.outcome.heading}>
          <p>{report.outcome.text}</p>
        </Box>
      ) : null}
    </section>
  );
}

/** The reasons for a tier, opened from its chip. Escape or the chip closes it. */
export function RiskPopover({ tier, initiallyOpen }: { tier: "low" | "medium" | "high"; initiallyOpen?: boolean }) {
  const [open, setOpen] = useState(Boolean(initiallyOpen));
  const id = useId();
  const root = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setOpen(false);
      root.current?.querySelector<HTMLElement>("button")?.focus();
    };
    document.addEventListener("keydown", close);
    return () => document.removeEventListener("keydown", close);
  }, [open]);
  return (
    <span className="crew-popover-root" ref={root}>
      <button
        type="button"
        className={`crew-risk crew-risk-${tier}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        onClick={() => setOpen((value) => !value)}
      >
        {TIER_LABEL[tier]}
      </button>
      {open ? (
        <div className="crew-pop" id={id} role="dialog" aria-label={`Why ${TIER_LABEL[tier]}`}>
          <h6>Why {TIER_LABEL[tier]}</h6>
          <dl className="crew-kv">
            {popoverRows.map((row) => (
              <div key={row.label}>
                <dt>{row.label}</dt>
                <dd>{row.value}</dd>
              </div>
            ))}
          </dl>
          <div className="crew-pop-acts">
            <Act label="How risk is scored" />
            <Act label="Open the diff" />
          </div>
        </div>
      ) : null}
    </span>
  );
}

function TaskCrumb({ title, tier, children }: { title: string; tier: "low" | "medium" | "high"; children?: ReactNode }) {
  return (
    <>
      <span className="crew-muted">All tasks</span>
      <b>{title}</b>
      {children ?? <RiskChip tier={tier} />}
    </>
  );
}

const TIER_RULE =
  "A score from the four reasons above, shown with its parts. A tier is never hidden, and you can see why it is what it is.";

/** risk-reasons: Low, Medium and High side by side. */
export function RiskReasonsScreen() {
  return (
    <CrewFrame tab="Flow" reviewCount={7} crumbs={<><span className="crew-muted">All tasks</span><b>Risk tiers</b></>}>
      <div className="crew-grid3">
        <RiskCard report={risk.lowReport} />
        <RiskCard report={risk.mediumReport} />
        <RiskCard report={risk.highReport} />
      </div>
      <div className="crew-pad">
        <Box heading="How the tier is decided">
          <p>{TIER_RULE}</p>
        </Box>
      </div>
    </CrewFrame>
  );
}

export function RiskPopoverScreen() {
  return (
    <CrewFrame tab="Flow" reviewCount={7} crumbs={<TaskCrumb title="Use next in the auth redirect" tier="medium"><RiskPopover tier="medium" initiallyOpen /></TaskCrumb>}>
      <div className="crew-pad crew-popover-space" />
    </CrewFrame>
  );
}

export function RiskHighFullScreen() {
  return (
    <CrewFrame tab="Flow" reviewCount={7} crumbs={<TaskCrumb title="Rotate the signing key" tier="high" />}>
      <Banner tone="plain" heading="Merged by Crew at 15:02." text="High risk under Autopilot Full: it is at the top of your Review queue, with a 4-hour review window." actions={[{ label: "Open in Review", primary: true }]} />
      <div className="crew-pad">
        <div className="crew-narrow520">
          <RiskCard report={risk.highFullReport} heading="Why High" />
        </div>
      </div>
    </CrewFrame>
  );
}

export function RiskHighOnScreen() {
  return (
    <CrewFrame tab="Flow" reviewCount={7} crumbs={<TaskCrumb title="Rotate the signing key" tier="high" />}>
      <Banner
        tone="amber"
        heading="Waiting for you before the merge."
        text="Autopilot is On for this repo, and this change is High risk."
        actions={[{ label: "Approve and merge", primary: true }, { label: "Send back to Build" }, { label: "Switch to Full" }]}
      />
      <div className="crew-pad">
        <div className="crew-narrow620">
          <RiskCard report={risk.highOnReport} heading="Why High" />
        </div>
      </div>
    </CrewFrame>
  );
}

export function RiskRaisedScreen() {
  return (
    <CrewFrame
      tab="Flow"
      reviewCount={7}
      crumbs={
        <TaskCrumb title="Use next in the auth redirect" tier="high">
          <RiskChip tier="high" />
          <Tag tone="hi">Raised from Medium</Tag>
        </TaskCrumb>
      }
    >
      <Banner
        tone="plain"
        heading="Raised to High risk."
        text="The Security review ran on Claude Code, the author’s engine, because no other vendor is signed in. That is weaker evidence, so Crew raised the tier."
        actions={[{ label: "Sign in to another engine", primary: true }]}
      />
      <div className="crew-pad">
        <div className="crew-narrow620">
          <RiskCard report={risk.raisedReport} heading="Why High" />
        </div>
      </div>
    </CrewFrame>
  );
}

export function RiskDemotedScreen() {
  return (
    <CrewFrame tab="Review" reviewCount={7} crumbs={<><span className="crew-muted">All tasks</span><b>Risk tiers</b></>}>
      <div className="crew-grid2">
        <RiskCard report={risk.demotedMedium} />
        <RiskCard report={risk.demotedLow} />
      </div>
      <div className="crew-pad">
        <Box heading="Where the area mode comes from">
          <p>
            Autopilot by area measures each area of the code and moves it between your ceiling and On. Open Settings › Crew › Autopilot by area for the record, the thresholds and the override.
          </p>
        </Box>
      </div>
    </CrewFrame>
  );
}

/** risk-board-card: each Taskboard card shows its tier and what happened to it. */
export function RiskBoardScreen() {
  return (
    <CrewFrame tab="Board" reviewCount={7} inbox={1}>
      <div className="crew-board">
        {board.map((column) => (
          <section key={column.label} className="crew-col" aria-label={column.label}>
            <div className={`crew-colh${column.tone === "warn" ? " crew-colh-warn" : ""}`}>
              <b>{column.label}</b>
              <span className="crew-count">{column.cards.length}</span>
            </div>
            {column.cards.map((card) => (
              <article key={card.title + column.label} className={`crew-tcard${card.needs ? " crew-tcard-needs" : ""}`}>
                <div className="crew-tk">
                  <b>implement</b> · Lead · now
                </div>
                <div className="crew-tt">{card.title}</div>
                <div className="crew-te">
                  <b>{card.engine}</b>
                  <span>{card.model}</span>
                </div>
                <div className="crew-tck">
                  <div className="crew-bar" aria-hidden>
                    <i style={{ width: "100%" }} />
                  </div>
                  {card.checks}
                </div>
                <div className="crew-tln">{card.paths}</div>
                {card.needs ? <div className="crew-tln crew-tln-amber">{card.line}</div> : null}
                <div className="crew-tln">
                  <RiskChip tier={card.tier} />
                  {card.needs ? null : <span>{card.line}</span>}
                </div>
              </article>
            ))}
          </section>
        ))}
      </div>
    </CrewFrame>
  );
}
