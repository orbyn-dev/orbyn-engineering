import type { ComponentType } from "react";
import { LiveCapacity } from "./LiveCapacity";
import type { CapacityLiveId } from "./names";

/** The screens on real data: the frames of Crew Designs v21 drawn from the capacity routes. */

export const CAPACITY_LIVE_SCREENS: Record<CapacityLiveId, ComponentType> = {
  "capacity-live-settings": () => <LiveCapacity view="settings" />,
  "capacity-live-coverage": () => <LiveCapacity view="coverage" />,
};
