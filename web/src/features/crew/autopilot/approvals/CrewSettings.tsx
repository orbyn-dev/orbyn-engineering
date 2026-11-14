import { useRoute } from "../../../../product/state/route";
import { AutonomyAreas } from "./AutonomyAreas";
import { AutonomyThresholds } from "./AutonomyThresholds";
import { AutopilotSettings } from "./AutopilotSettings";
import { ProductionTargets } from "./ProductionTargets";

/** Settings › Crew: Autopilot, then the production-linked targets the guard watches. Autopilot by area and its thresholds are pages of their own. */
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
            <ProductionTargets />
          </>
        )}
      </div>
    </>
  );
}
