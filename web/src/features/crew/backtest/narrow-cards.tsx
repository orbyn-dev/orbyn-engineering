import { Planned } from "../capacity/Planned";

/** Why the replay count is not a figure: the report route does not serve one per lane. */
const REPLAYS = "the report route does not serve how many pull requests each lane replayed";

/** The run's own card: its state, day and outcome, the lanes that scored, and each engine's tokens. */
const RUN = {
  outcome: "Partly measured",
  state: "Finished",
  day: "Sat 7 Nov",
  lines: [
    "Outcomes: the lanes the log could not score show finding counts only",
    "Lanes that scored: 2 of 3",
    "Used: claude 1.28M tokens · codex 896k · opencode 396k",
  ],
};

/** One card per lane, as the service names it: no display name, vendor or Checks lane is served. */
const LANES = [
  {
    title: "correctness · claude · sonnet-5-5",
    state: "Done",
    lines: ["Escapes caught: 6 of 9 · recall 66%", "Findings raised: 21 · 2 unresolved", "Matched an escape: 6 of 21 · precision 28%"],
  },
  {
    title: "correctness · codex · gpt-5-5",
    state: "Done",
    lines: ["Escapes caught: 5 of 9 · recall 55%", "Findings raised: 17 · 0 unresolved", "Matched an escape: 5 of 17 · precision 29%"],
  },
  {
    title: "security · opencode · deepseek-v4",
    state: "Done",
    lines: ["Escapes caught: Not measured · the escape log can’t say", "Findings raised: 9", "Matched an escape: Not measured · the escape log can’t say"],
  },
];

/** The report body both narrow frames draw (Crew Designs v95): only what the report serves. */
export function NarrowReportCards({ tail }: { tail: string }) {
  return (
    <>
      <div className="cx-t14card">
        <b>{RUN.outcome}</b>
        <div className="cx-t14r">
          <span className="cx-apchip">{RUN.state}</span>
          <span className="cx-apchip">{RUN.day}</span>
        </div>
        {RUN.lines.map((line) => (
          <small key={line}>{line}</small>
        ))}
      </div>
      {LANES.map((lane) => (
        <div className="cx-t14card" key={lane.title}>
          <b>{lane.title}</b>
          <div className="cx-t14r">
            <span className="cx-apchip">{lane.state}</span>
          </div>
          {lane.lines.map((line) => (
            <small key={line}>{line}</small>
          ))}
        </div>
      ))}
      <p style={{ margin: 0, display: "flex", alignItems: "center", gap: "6px", fontSize: "11.5px", color: "var(--color-muted)", lineHeight: 1.4 }}>
        <Planned why={REPLAYS} style={{ marginLeft: 0, flex: "none", whiteSpace: "nowrap" }} />
        <span>Replays per lane are not served.</span>
      </p>
      <span style={{ color: "var(--color-muted)", fontSize: "11.5px" }}>{tail}</span>
    </>
  );
}
