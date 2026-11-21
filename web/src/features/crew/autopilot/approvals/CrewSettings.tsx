import { useRoute } from "../../../../product/state/route";
import { LiveBacktest } from "../../backtest/live/LiveBacktest";
import { CapacityEntry } from "../../capacity/live/CapacityEntry";
import { LiveCapacity } from "../../capacity/live/LiveCapacity";
import { LiveEscapes } from "../../escape/live/LiveEscapes";
import { AutonomyAreas } from "./AutonomyAreas";
import { AutonomyThresholds } from "./AutonomyThresholds";
import { AutopilotSettings } from "./AutopilotSettings";
import { BacktestEntry, EscapesEntry, OracleEntry } from "./CrewEntries";
import { PathClasses } from "./PathClasses";
import { ProductionTargets } from "./ProductionTargets";
import { EngineTiers } from "./EngineTiers";
import { WorkerProvider } from "./WorkerProvider";

/** Settings › Crew: Autopilot, the production-linked targets the guard watches, then the path classes of the repo. Autopilot by area, its thresholds, Review capacity (and its coverage map), the escape log, the reviewer backtest and its estimate and report are pages of their own. */
export function CrewSettingsSection() {
  const route = useRoute();
  const page = route.name === "settings" ? route.crewPage : undefined;
  const repo = route.name === "settings" ? route.repo : undefined;
  const id = route.name === "settings" ? route.id : undefined;
  return (
    <>
      <h1 className="sr-only" tabIndex={-1} data-page-title>Crew</h1>
      <div className="ap-stack">
        {page === "areas" ? (
          <AutonomyAreas />
        ) : page === "thresholds" ? (
          <AutonomyThresholds />
        ) : page === "capacity" ? (
          <LiveCapacity view="settings" />
        ) : page === "coverage" ? (
          <LiveCapacity view="coverage" initial={repo} />
        ) : page === "escapes" ? (
          <LiveEscapes view="log" />
        ) : page === "escape" ? (
          <LiveEscapes view="detail" id={id} />
        ) : page === "backtest" ? (
          <LiveBacktest view="page" />
        ) : page === "estimate" ? (
          <LiveBacktest view="estimate" />
        ) : page === "report" ? (
          <LiveBacktest view="report" id={id} />
        ) : (
          <>
            <AutopilotSettings />
            <CapacityEntry />
            <EscapesEntry />
            <OracleEntry />
            <BacktestEntry />
            <WorkerProvider />
            <EngineTiers />
            <ProductionTargets />
            <PathClasses />
          </>
        )}
      </div>
    </>
  );
}
