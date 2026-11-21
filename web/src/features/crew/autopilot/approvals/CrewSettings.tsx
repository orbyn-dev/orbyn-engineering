import { useRoute } from "../../../../product/state/route";
import { CapacityEntry } from "../../capacity/live/CapacityEntry";
import { LiveCapacity } from "../../capacity/live/LiveCapacity";
import { AutonomyAreas } from "./AutonomyAreas";
import { AutonomyThresholds } from "./AutonomyThresholds";
import { AutopilotSettings } from "./AutopilotSettings";
import { PathClasses } from "./PathClasses";
import { ProductionTargets } from "./ProductionTargets";
import { EngineTiers } from "./EngineTiers";
import { WorkerProvider } from "./WorkerProvider";

/** Settings › Crew: Autopilot, the production-linked targets the guard watches, then the path classes of the repo. Autopilot by area, its thresholds and Review capacity are pages of their own. */
export function CrewSettingsSection() {
  const route = useRoute();
  const page = route.name === "settings" ? route.crewPage : undefined;
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
        ) : (
          <>
            <AutopilotSettings />
            <CapacityEntry />
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
