import { AutopilotSettings } from "./AutopilotSettings";
import { PathClasses } from "./PathClasses";
import { ProductionTargets } from "./ProductionTargets";

/** Settings › Crew: Autopilot, the production-linked targets the guard watches, then the path classes of the repo. */
export function CrewSettingsSection() {
  return (
    <>
      <h1 className="sr-only" tabIndex={-1} data-page-title>Crew</h1>
      <div className="ap-stack">
        <AutopilotSettings />
        <ProductionTargets />
        <PathClasses />
      </div>
    </>
  );
}
