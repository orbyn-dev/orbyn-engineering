import { useRoute } from "../../../../product/state/route";
import { AutonomyAreas } from "./AutonomyAreas";
import { AutonomyThresholds } from "./AutonomyThresholds";
import { AutopilotSettings } from "./AutopilotSettings";
import { DigestTime } from "./DigestTime";
import { PathClasses } from "./PathClasses";
import { ProductionTargets } from "./ProductionTargets";
import { EngineTiers } from "./EngineTiers";
import { WorkerProvider } from "./WorkerProvider";

/** Settings › Crew: Autopilot, the production-linked targets the guard watches, then the path classes of the repo. Autopilot by area and its thresholds are pages of their own. */
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
        ) : (
          <>
            <AutopilotSettings />
            <DigestTime />
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
