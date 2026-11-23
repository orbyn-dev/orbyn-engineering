import { Planned } from "../capacity/Planned";
import { Frame } from "../shared/Frame";
import { Btn, Ic, PageHead, Seg } from "../shared/parts";

/** Why the report and its figures are not built, in the words their pills give. */
const WHY_REPORT = "GET /api/crew/escapes/report (per week, engine, area and lane, rates, gap and check counts) is not built";

const ENGINES = ["claude · Sonnet 5.5", "codex · GPT-5.5", "opencode · DeepSeek V4"];
const AREAS = ["web/auth", "docs/guides", "web/app"];

/** One column of "Not measured": no report read exists, so no figure is invented, not even a zero. */
const Measured = () => <b>Not measured</b>;

/** A "By engine" or "By area" table: the design's sample names, every figure Not measured. */
function NotMeasuredTable({ heading, names }: { heading: string; names: readonly string[] }) {
  return (
    <>
      <h5 className="cx-t10h5">
        {heading}
        <Planned why={WHY_REPORT} />
        <span className="cx-t10r">Open the escape log</span>
      </h5>
      <div className="cx-aptb" style={{ gridTemplateColumns: "minmax(0,1.3fr) minmax(0,1fr) minmax(0,1.4fr) minmax(0,1fr)" }}>
        <div className="cx-h">{heading === "By engine" ? "Engine" : "Area"}</div>
        <div className="cx-h">Escaped</div>
        <div className="cx-h">How</div>
        <div className="cx-h">Checks from them</div>
        {names.map((name) => (
          <NameRow key={name} name={name} />
        ))}
      </div>
    </>
  );
}

function NameRow({ name }: { name: string }) {
  return (
    <>
      <div><b>{name}</b></div>
      <div><Measured /></div>
      <div><Measured /></div>
      <div><Measured /></div>
    </>
  );
}

/**
 * Crew Designs v79, frame `escape-dashboard`. The report it needs (GET /api/crew/escapes/report) is not built, so every
 * figure here reads Not measured, never a zero and never a rate: the engine and area names are the design's sample, and
 * the log the dashboard would count is the built one. Nothing here reads or sends anything.
 */
export function EscapeDashboard() {
  return (
    <Frame>
      <div className="cx-cwpg">
        <PageHead sub="Login redirect crew · Lead: Fix login redirect on claude · 3 workers" acts />
        <div className="cx-cwtabs">
          <span className="cx-segx">
            {["Approvals", "Trust", "Background", "Inbox", "Flow", "Board", "Map", "Lanes"].map((label) => (
              <span key={label}>{label}</span>
            ))}
            <span className="cx-on">
              Review
              <Planned why="the beta has no Review tab; this view is drawn here and is reached another way" style={{ height: "16px", verticalAlign: "middle" }} />
            </span>
          </span>
          <span className="cx-end">claude 412k of 1M tokens · codex 62% of its weekly limit · 2h 14m</span>
        </div>
        <div className="cx-cwtool">
          <Seg kind="cwseg" on={4} items={["Review queue · 7", "Merge queue · 3", "Digest", "Trust", "Dashboard", "Background"]} />
          <span style={{ marginLeft: "auto" }}>Main is healthy · watched since 09:00</span>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Dashboard</b>
            <span className="cx-rtag cx-lo">Sample data</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <span className="cx-t10sel"><Ic n="folder" sm />orbyn-engineering<Ic n="down" sm /></span>
              <Seg kind="cwseg" on={1} items={["30 days", "60 days", "90 days"]} />
            </span>
          </div>
          <div className="cx-t10g cx-t10two" style={{ gridTemplateColumns: "minmax(0,1.6fr) minmax(0,1fr)" }}>
            <div className="cx-t10c">
              <span className="cx-t10l">Escapes by week</span>
              <span className="cx-t10b">Not measured</span>
              <span className="cx-t10s">reverted or found after the merge</span>
              <span className="cx-t10f">
                The weekly figures come with the report <Planned why={WHY_REPORT} />. The escape log already counts them,
                apart from each other.
              </span>
            </div>
            <div className="cx-t10c">
              <span className="cx-t10l">Escape rate</span>
              <span className="cx-t10b">Not measured</span>
              <span className="cx-t10s">of merges got past review and checks</span>
              <Btn className="cx-ghost" style={{ height: "26px", alignSelf: "flex-start", marginTop: "6px" }}>Open the escape log</Btn>
              <span className="cx-t10f">Last 60 days</span>
            </div>
          </div>
          <NotMeasuredTable heading="By engine" names={ENGINES} />
          <NotMeasuredTable heading="By area" names={AREAS} />
          <div className="cx-fbx">
            <h6>Why there is no percentage</h6>
            <p>
              No rate is shown in v1: the log counts escapes, and the rate comes with the report{" "}
              <Planned why={WHY_REPORT} />. When it exists, a rate will show from 10 merges and count confirmed escapes only
              (possible ones stay apart), because a rate on a handful of merges mostly says luck.
            </p>
          </div>
        </div>
      </div>
    </Frame>
  );
}
