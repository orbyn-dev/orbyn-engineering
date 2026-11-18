/** Ids of the capacity screens on real data. Each opens at `#/preview/crew/<id>`. */
export const CAPACITY_LIVE_FRAMES = ["capacity-live-settings", "capacity-live-coverage"] as const;
export type CapacityLiveId = (typeof CAPACITY_LIVE_FRAMES)[number];
