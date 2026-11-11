import { AutopilotSettings } from "./AutopilotSettings";
import { ProductionTargets } from "./ProductionTargets";

/** Settings › Crew: Autopilot, then the production-linked targets the guard watches. */
export function CrewSettingsSection() {
  return (
    <>
      <h1 className="sr-only" tabIndex={-1} data-page-title>Crew</h1>
      <div className="ap-stack">
        <AutopilotSettings />
        <ProductionTargets />
      </div>
    </>
  );
}
