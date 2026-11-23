import { useEffect, useState, type ComponentType } from "react";
import { Frame } from "../shared/Frame";
import { Dashboard } from "./Dashboard";
import type { DashboardClient } from "./contract";
import { ALL } from "./prefs";
import type { DashboardFrameId } from "./names";

type Setup = {
  mock?: { delayMs?: number; failAll?: boolean; never?: boolean };
  project?: string;
  metric?: "autonomy" | "revert";
  menu?: boolean;
  compact?: boolean;
  width?: number;
};

/** What each frame of "23. Dashboard" starts from. The page is the real one; only the client behind it is the sample. */
const SETUP: Record<DashboardFrameId, Setup> = {
  "dash-default": {},
  "dash-metric": { metric: "revert" },
  "dash-breakdown": {},
  "dash-low-sample": { project: "orbyn-planner" },
  "dash-empty": { project: "orbyn-engine" },
  "dash-many-repos": { project: ALL },
  "dash-filters": { menu: true },
  "dash-loading": { mock: { never: true } },
  "dash-error": { mock: { failAll: true } },
  "dash-narrow-panel": { compact: true },
  "dash-narrow-window": { width: 640 },
  "dash-area-history": {},
};

function DashFrame({ id }: { id: DashboardFrameId }) {
  const setup = SETUP[id];
  // The sample client is dev-only and loaded on demand, so this chunk is the only place that names it.
  const [sample, setSample] = useState<{ client: DashboardClient; projects: readonly { id: string; name: string }[] } | null>(null);
  useEffect(() => {
    let live = true;
    void import("../../../../mock/crewDashboard").then((module) => {
      if (live) setSample({ client: module.createCrewDashboardMock(setup.mock), projects: module.SAMPLE_PROJECTS });
    });
    return () => { live = false; };
  }, [setup.mock]);
  return (
    <Frame sheet panel={setup.compact} width={setup.width}>
      {sample ? <Dashboard sample compact={setup.compact} projects={sample.projects} client={sample.client} initial={{ project: setup.project ?? "orbyn-engineering", metric: setup.metric, menu: setup.menu }} /> : null}
    </Frame>
  );
}

const frame = (id: DashboardFrameId): ComponentType => () => <DashFrame id={id} />;

export const DASHBOARD_SCREENS: Record<DashboardFrameId, ComponentType> = {
  "dash-default": frame("dash-default"),
  "dash-metric": frame("dash-metric"),
  "dash-breakdown": frame("dash-breakdown"),
  "dash-low-sample": frame("dash-low-sample"),
  "dash-empty": frame("dash-empty"),
  "dash-many-repos": frame("dash-many-repos"),
  "dash-filters": frame("dash-filters"),
  "dash-loading": frame("dash-loading"),
  "dash-error": frame("dash-error"),
  "dash-narrow-panel": frame("dash-narrow-panel"),
  "dash-narrow-window": frame("dash-narrow-window"),
  "dash-area-history": frame("dash-area-history"),
};
