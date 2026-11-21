import { useEffect, useState } from "react";
import { useWaited } from "../../../lib/motion";
import { routeHref } from "../../../product/state/route";
import { AllRepos } from "./AllRepos";
import { AreaModes } from "./AreaModes";
import { PeriodChoice, ProjectMenu, type Choosable } from "./Controls";
import { Failure } from "./Failure";
import { Bars, Planned, WHY } from "./parts";
import { AreaTable, EngineTable } from "./Tables";
import { Tiles, type Metric } from "./Tiles";
import { PERIODS, dashboardClient, type DashboardClient, type Period } from "./contract";
import { CHECKS_TEXT, NEEDED_TEXT, dayLong, mergesOf, text, type DashModel } from "./model";
import { ALL, readChoice, writeChoice } from "./prefs";
import { useModel } from "./useDashboard";
import "./dashboard.css";

export interface DashboardProps {
  /** The repositories Crew can be asked about. */
  projects: readonly Choosable[];
  /** The reads. The app's own client when absent; the Preview frames hand in the mock. */
  client?: DashboardClient;
  /** The Preview frames: the figures are samples, and nothing is remembered. */
  sample?: boolean;
  /** The 320 px panel: one column, tiles only. */
  compact?: boolean;
  /** A project the address named. */
  wanted?: string;
  /** Where the page starts, for the Preview frames. */
  initial?: { project?: string; metric?: Metric; menu?: boolean };
}

const known = (id: string | null | undefined, projects: readonly Choosable[]): id is string => typeof id === "string" && (id === ALL || projects.some((entry) => entry.id === id));

function Skeleton() {
  return (
    <div className="cx-t10g" aria-hidden>
      {["Autonomy", "Cycle time", "Cost", "Revert rate"].map((label) => (
        <div className="cx-t10c" key={label}>
          <span className="cx-t10l">{label}</span>
          <span className="cx-t10sk" /><span className="cx-t10sk" /><span className="cx-t10sk" />
        </div>
      ))}
    </div>
  );
}

function Unread() {
  return (
    <div className="cx-t10g" data-testid="dashboard-tiles">
      {["Autonomy", "Cycle time", "Cost", "Revert rate"].map((label) => (
        <div className="cx-t10c" key={label}>
          <span className="cx-t10l">{label}</span>
          <span className="cx-t10b">—</span>
          <span className="cx-t10f">Not shown while the history can’t be read</span>
        </div>
      ))}
    </div>
  );
}

function Empty({ name }: { name: string }) {
  return (
    <div className="cx-cwstate" style={{ margin: "32px auto" }}>
      <b>No merges yet</b>
      <p>When Crew merges a change in {name}, this page shows how often it needed you, how long it took, what it cost per engine and whether it was reverted. Each figure appears with the number of merges it rests on.</p>
      <span style={{ display: "flex", alignItems: "center", gap: "6px" }}>
        <Planned why={WHY.queue} />
        <button type="button" className="cx-ghost" disabled title={`Planned: ${WHY.queue}`} aria-description={`Planned: ${WHY.queue}`}>Open the Review queue</button>
      </span>
    </div>
  );
}

function RecentMerges({ model }: { model: DashModel }) {
  if (!model.recent || model.recent.length === 0) return null;
  return (
    <>
      <h5 className="cx-t10h5">The {model.recent.length} {model.recent.length === 1 ? "merge" : "merges"}</h5>
      <div className="cx-t10list" data-testid="recent-merges">
        {model.recent.map((merge, at) => (
          <div key={`${merge.title}-${at}`}>
            <span>{merge.title}<small>{merge.engine} · {NEEDED_TEXT.get(merge.needed) ?? "you took part"}</small></span>
            <span>{CHECKS_TEXT.get(merge.checks) ?? "No checks"}</span>
            <span className="cx-t10t">{dayLong(merge.at)}</span>
          </div>
        ))}
      </div>
    </>
  );
}

function HowToRead({ model }: { model: DashModel }) {
  return (
    <div className="cx-fbx">
      <h6>How to read this</h6>
      <p>
        A rate shows from {model.minRate} merges, over the period you pick, per repo, on this device: runs made on other devices are not included. Autonomy counts merges that needed nothing from you after the brief; tasks that were stopped or cancelled are not part of it. Cycle time runs from the brief to the merge, and a median needs {model.minMedian} merges. Cost is shown per engine in its own unit and is never added into one figure.
        {model.source === "trust" ? " Autonomy, cycle time, cost and the bars by week are Planned: they wait for a read the service does not have yet." : ""}
      </p>
    </div>
  );
}

/** One metric by week, with the sample under every bar. No week is turned into a percentage: none has enough merges for one. */
function MetricView({ model, metric, onBack }: { model: DashModel; metric: Metric; onBack(): void }) {
  const weeks = model.weeks ?? [];
  const reverts = model.reverts ?? [];
  const title = metric === "revert" ? "Revert rate" : "Autonomy";
  return (
    <>
      <div className="cx-frh">
        <b>{title}</b>
        <span className="cx-rtag">{mergesOf(model.merges)} · {model.days} days</span>
        <button type="button" className="cx-t10r" style={{ marginLeft: "auto", textDecoration: "underline", textUnderlineOffset: "2px", color: "var(--color-textSoft)" }} onClick={onBack}>All metrics</button>
      </div>
      <div className={metric === "revert" ? "cx-t10g cx-t10two" : "cx-t10g cx-t10two cx-t10one"}>
        <div className="cx-t10c">
          <span className="cx-t10l">{metric === "revert" ? "Reverted merges by week" : "Merges by week that needed nothing from you"}</span>
          <Bars weeks={weeks} mode={metric === "revert" ? "reverted" : "unaided"} big />
          <span className="cx-t10f">
            Each bar is a week: its height and the figure under it are the merges that week; the filled part is the merges that {metric === "revert" ? "were reverted" : "needed nothing from you"}. Weeks with fewer than {model.minRate} merges are not turned into percentages.
          </span>
        </div>
        {metric === "revert" ? (
          <div className="cx-t10c">
            <span className="cx-t10l">{reverts.length === 1 ? "The revert" : `The ${reverts.length} reverts`}</span>
            {reverts.length === 0 ? (
              <span className="cx-t10s">No merge was reverted in the last {model.days} days.</span>
            ) : (
              <div className="cx-t10list">
                {reverts.map((revert, at) => (
                  <div key={`${revert.title}-${at}`}>
                    <span>{revert.title}<small>{revert.by === "person" ? "Reverted by you" : revert.by === "auto" ? `Reverted automatically${revert.certain === true ? " · cause certain" : ""}` : "Reverted"}</small></span>
                    <span className="cx-t10t">{dayLong(revert.at)}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        ) : null}
      </div>
      <HowToRead model={model} />
    </>
  );
}

function ProjectView({ client, id, name, days, compact, metric, onMetric, onWindow }: { client: DashboardClient; id: string; name: string; days: Period; compact: boolean; metric: Metric | null; onMetric(next: Metric | null): void; onWindow(days: number): void }) {
  const read = useModel(client, id, days);
  const slow = useWaited(read.status === "loading");
  const readDays = read.status === "ready" ? read.data.days : null;
  useEffect(() => {
    if (readDays !== null && readDays > 0) onWindow(readDays);
  }, [readDays, onWindow]);
  if (read.status === "loading") return slow ? <Skeleton /> : <span className="sr-only" role="status">Reading the history</span>;
  if (read.status === "error") return <><Failure failure={read.error} onRetry={() => void read.reload()} /><Unread /></>;
  const model = read.data;
  if (model.merges === 0) return <Empty name={name} />;
  if (metric && model.weeks && model.weeks.length > 0 && !compact) return <MetricView model={model} metric={metric} onBack={() => onMetric(null)} />;
  const few = model.merges < model.minRate;
  return (
    <>
      {few ? (
        <div className="cx-cwbn" style={{ margin: 0 }} role="note">
          <span>This repo has {mergesOf(model.merges)} in the last {model.days} days. A rate shows from {model.minRate} merges, so each tile says what it rests on{model.recent && model.recent.length > 0 ? " and Crew lists the merges themselves" : ""}.</span>
        </div>
      ) : null}
      <Tiles model={model} onOpen={onMetric} compact={compact} />
      {compact ? (
        <a className="cx-ghost" style={{ alignSelf: "flex-start" }} href={routeHref({ name: "settings", section: "crew", crewPage: "dashboard", repo: id })}>Open in Crew</a>
      ) : (
        <>
          {few ? <RecentMerges model={model} /> : null}
          {model.weeks === null ? (
            <h5 className="cx-t10h5">Over time <Planned why={WHY.weeks} /></h5>
          ) : model.weeks.length > 0 ? (
            <>
              <h5 className="cx-t10h5">Over time<span className="cx-t10r" style={{ textDecoration: "none" }}>Autonomy, by week</span></h5>
              <div className="cx-t10c">
                <Bars weeks={model.weeks} mode="unaided" big />
                <span className="cx-t10f">Each bar is a week. Its height and the figure under it are the merges that week; the filled part is the merges that needed nothing from you.</span>
              </div>
            </>
          ) : null}
          <EngineTable model={model} />
          <AreaTable model={model} />
          <AreaModes client={client} project={id} days={model.days || days} />
          <HowToRead model={model} />
        </>
      )}
    </>
  );
}

/**
 * The Crew dashboard (parity W12, Crew Designs "23. Dashboard"): whether Crew is paying off, per repository, over a period. It draws what the reads
 * hold and a Planned mark where they hold nothing: no figure is made up and none is zero for want of data. Every tile says the number of merges it rests
 * on, a rate shows only from the minimum sample, and cost stays per engine in its own unit.
 */
export function Dashboard({ projects, client: given, sample, compact, wanted, initial }: DashboardProps) {
  const client = given ?? dashboardClient();
  const [kept] = useState(() => (sample ? { project: null, days: null } : readChoice()));
  const [project, setProject] = useState<string>(() => [wanted, initial?.project, kept.project].find((id) => known(id, projects)) ?? projects[0]?.id ?? "");
  const [days, setDays] = useState<Period>(kept.days ?? 60);
  const [metric, setMetric] = useState<Metric | null>(initial?.metric ?? null);
  const [window_, setWindow] = useState<number | null>(null);
  const fixed = client.history === null;
  const shown = fixed ? (window_ ?? 60) : days;
  const choose = (next: string, period: Period) => {
    setProject(next);
    setDays(period);
    setMetric(null);
    if (!sample && next !== "") writeChoice({ project: next, days: period });
  };
  const current = projects.find((entry) => entry.id === project);
  return (
    <section className="cx-dash" aria-label="Crew dashboard" data-compact={compact ? "" : undefined}>
      <div className="cx-t10hd">
        <h2>Dashboard</h2>
        {sample ? <span className="cx-rtag cx-lo">Sample data</span> : null}
        <span className="cx-rtag cx-lo">This device only</span>
        <span className="cx-t10sp">
          <ProjectMenu projects={projects} current={project} onPick={(id) => choose(id, days)} open={initial?.menu} />
          <PeriodChoice days={shown} fixed={fixed} onPick={(period) => choose(project, period)} />
        </span>
      </div>
      {projects.length === 0 ? (
        <small className="cx-t10f">There is no project to show.</small>
      ) : project === ALL ? (
        <AllRepos client={client} projects={projects} days={PERIODS.find((period) => period === shown) ?? 60} onOpen={(id) => choose(id, days)} />
      ) : (
        <ProjectView
          key={`${project}:${days}`}
          client={client}
          id={project}
          name={text(current?.name) || "this project"}
          days={days}
          compact={compact === true}
          metric={metric}
          onMetric={setMetric}
          onWindow={setWindow}
        />
      )}
    </section>
  );
}
