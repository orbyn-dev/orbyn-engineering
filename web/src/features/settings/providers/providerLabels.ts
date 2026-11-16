import type { ProviderLabel } from "../../../generated/ProviderLabel";

/** The words each label carries, the same in the list and the detail (provider-states). */
export const LABEL_TEXT: Record<ProviderLabel, string> = {
  not_verified: "Not verified",
  unmanaged: "Unmanaged",
  limited_support: "Limited support",
  cant_start: "Can’t start",
  planned: "Planned",
  not_found: "Not found",
};

/** The groups the page draws, in order (provider-list, provider-list-more, provider-extras). "In use" is the
 * per-project switch (s-providers): a startable engine switched on is In use, one switched off sits under its own group. */
export type PlaceKey = "in_use" | "found" | "more" | "cant_start" | "not_found" | "planned";

export const GROUPS: { place: PlaceKey; heading: string; note?: string }[] = [
  { place: "in_use", heading: "In use" },
  { place: "found", heading: "Found on this computer" },
  { place: "more", heading: "More engines", note: "These start over ACP when Orbyn finds their program. Each was tried so far with a test agent, so launch, models and approvals are not verified." },
  { place: "cant_start", heading: "Found, can’t start" },
  { place: "not_found", heading: "Not found" },
  { place: "planned", heading: "Not built yet" },
];
