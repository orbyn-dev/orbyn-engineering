import { useRoute } from "../../../../product/state/route";
import { LiveBacktest } from "../../backtest/live/LiveBacktest";
import { CapacityEntry } from "../../capacity/live/CapacityEntry";
import { LiveCapacity } from "../../capacity/live/LiveCapacity";
import { LiveDashboard } from "../../dashboard/LiveDashboard";
import { LiveEscapeSettings } from "../../escape/live/LiveEscapeSettings";
import { LiveEscapes } from "../../escape/live/LiveEscapes";
import { AutonomyAreas } from "./AutonomyAreas";
import { AutonomyThresholds } from "./AutonomyThresholds";
import { AutopilotSettings } from "./AutopilotSettings";
import { DigestTime } from "./DigestTime";
import { BacktestEntry, DashboardEntry, EscapesEntry, OracleEntry } from "./CrewEntries";
import { PathClasses } from "./PathClasses";
import { ProductionTargets } from "./ProductionTargets";
import { ProfilesSettings } from "../../../profiles";
import { EngineTiers } from "./EngineTiers";
import { WorkerProvider } from "./WorkerProvider";
import { CrewDiskFloor } from "./CrewDiskFloor";
import { LessonsEntry } from "../../lessons/LessonsEntry";
import { LiveLessons } from "../../lessons/LiveLessons";
import { LessonAddPage } from "../../lessons/LessonAddPage";
import { LessonCardPage } from "../../lessons/LessonCardPage";
import { SkillProposalPage } from "../../lessons/proposals/SkillProposalPage";

/** Settings › Crew: Autopilot, the production-linked targets the guard watches, then the path classes of the repo. Autopilot by area, its thresholds, Review capacity (and its coverage map), the Dashboard, the escape log, the lessons list, the reviewer backtest and its estimate and report are pages of their own. */
export function CrewSettingsSection() {
  const route = useRoute();
  const page = route.name === "settings" ? route.crewPage : undefined;
  const repo = route.name === "settings" ? route.repo : undefined;
  const id = route.name === "settings" ? route.id : undefined;
  const from = route.name === "settings" ? route.from : undefined;
  return (
    <>
      <div className="ap-stack">
        {page === "areas" ? (
          <AutonomyAreas />
        ) : page === "thresholds" ? (
          <AutonomyThresholds />
        ) : page === "capacity" ? (
          <LiveCapacity view="settings" initial={repo} />
        ) : page === "coverage" ? (
          <LiveCapacity view="coverage" initial={repo} from={from} />
        ) : page === "dashboard" ? (
          <LiveDashboard wanted={repo} />
        ) : page === "escapes" ? (
          <LiveEscapes view="log" initial={repo} />
        ) : page === "possible" ? (
          <LiveEscapes view="possible" initial={repo} />
        ) : page === "escape-settings" ? (
          <LiveEscapeSettings />
        ) : page === "escape" ? (
          <LiveEscapes view="detail" id={id} />
        ) : page === "backtest" ? (
          <LiveBacktest view="page" />
        ) : page === "estimate" ? (
          <LiveBacktest view="estimate" />
        ) : page === "report" ? (
          <LiveBacktest view="report" id={id} />
        ) : page === "lessons" ? (
          <LiveLessons view="list" initial={repo} />
        ) : page === "lesson" ? (
          <LessonCardPage id={id} repo={repo} />
        ) : page === "lesson-add" ? (
          <LessonAddPage repo={repo} />
        ) : page === "skill-proposal" ? (
          <SkillProposalPage id={id} repo={repo} />
        ) : (
          <>
            <ProfilesSettings />
            <AutopilotSettings />
            <DigestTime />
            <CapacityEntry />
            <DashboardEntry />
            <EscapesEntry />
            <LessonsEntry />
            <OracleEntry />
            <BacktestEntry />
            <CrewDiskFloor />
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
