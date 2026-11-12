import type { ProviderGroup } from "../../../generated/ProviderGroup";
import type { ProviderLabel } from "../../../generated/ProviderLabel";

/** The words each label carries, the same in the list and the detail (provider-states). */
export const LABEL_TEXT: Record<ProviderLabel, string> = {
  not_verified: "Not verified",
  unmanaged: "Unmanaged",
  limited_support: "Limited support",
  cant_start: "Can’t start",
  planned: "Planned",
};

/** The heading over each group, in the order the page draws them (provider-list, provider-list-more). */
export const GROUPS: { group: ProviderGroup; heading: string }[] = [
  { group: "in_use", heading: "In use" },
  { group: "found", heading: "Found on this computer" },
  { group: "cant_start", heading: "Found, can’t start" },
  { group: "planned", heading: "Not built yet" },
];
