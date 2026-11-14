import type { RiskReport } from "./model";

/** Sample data for the risk frames (Crew Designs v19). Area counts are examples. */

export const lowReport: RiskReport = {
  tier: "low",
  title: "Update the login docs",
  reasons: [
    { icon: "paths", label: "Paths", value: "docs/login.md", effect: "lowers" },
    { icon: "size", label: "Size", value: "1 file · +6 −2", effect: "lowers" },
    { icon: "coverage", label: "Coverage", value: "Docs build and link check cover it", effect: "lowers" },
    { icon: "revert-rate", label: "Revert rate", value: "docs/ · 0 of 31 · last 60 days", effect: "lowers" },
  ],
  outcome: { heading: "Merges by itself", text: "Review it in a batch whenever you like." },
};

export const mediumReport: RiskReport = {
  tier: "medium",
  title: "Use next in the auth redirect",
  reasons: [
    { icon: "paths", label: "Paths", value: "auth/redirect.ts (an auth path)", effect: "raises" },
    { icon: "size", label: "Size", value: "2 files · +18 −4", effect: "none" },
    { icon: "coverage", label: "Coverage", value: "84% of touched lines", effect: "lowers" },
    { icon: "revert-rate", label: "Revert rate", value: "auth/ · 2 of 14 · last 60 days", effect: "raises" },
    { icon: "area-mode", label: "Area mode", value: "Area auth is on Full: 1 revert in 14 days · 8 merges", effect: "none" },
  ],
  outcome: { heading: "Merges by itself · review within 24 hours", text: "It reaches your Review queue after the merge, with its proof." },
};

export const highReport: RiskReport = {
  tier: "high",
  title: "Rotate the signing key",
  reasons: [
    { icon: "paths", label: "Paths", value: "auth/login.ts, keys/rotate.ts", effect: "raises" },
    { icon: "size", label: "Size", value: "6 files · +412 −88", effect: "raises" },
    { icon: "coverage", label: "Coverage", value: "61% of touched lines", effect: "raises" },
    { icon: "revert-rate", label: "Revert rate", value: "keys/ · 3 of 11 · last 60 days", effect: "raises" },
  ],
  outcome: { heading: "Merges by itself under Full · review within 4 hours", text: "Under On, a High change waits for you before it merges." },
};

/** Why High under Full: the reasons list includes the area's mode, and what was not verified. */
export const highFullReport: RiskReport = {
  ...highReport,
  reasons: [
    { icon: "paths", label: "Paths", value: "auth/login.ts, keys/rotate.ts", effect: "raises" },
    { icon: "size", label: "Size", value: "6 files · +412 −88", effect: "raises" },
    { icon: "coverage", label: "Coverage", value: "61% of touched lines", effect: "raises" },
    { icon: "area-mode", label: "Area mode", value: "Area keys is on Full: 0 reverts in 14 days · 5 merges", effect: "none" },
  ],
  outcome: { heading: "Not verified", text: "Key rotation under load. No check covers it." },
};

export const highOnReport: RiskReport = {
  ...highReport,
  // Crew Designs v37: Paths, Size, Revert rate, and Area mode as planned (it comes after the beta and is not an input to the tier).
  reasons: [
    ...highReport.reasons.filter((reason) => reason.icon !== "coverage"),
    { icon: "area-mode", label: "Area mode", value: "Planned: after the autonomy-by-area slice", effect: "none", muted: true },
  ],
  outcome: {
    heading: "Everything before the merge is green",
    text: "Checks 5 of 5, Security (Codex) 0 confirmed, Finding check clear. Not verified: key rotation under load.",
  },
};

export const raisedReport: RiskReport = {
  tier: "high",
  title: "Use next in the auth redirect",
  raisedFrom: "medium",
  reasons: [
    { icon: "security", label: "Security review", value: "Same engine as author · lead tier", effect: "raises" },
    { icon: "paths", label: "Paths", value: "auth/redirect.ts (an auth path)", effect: "raises" },
    { icon: "size", label: "Size", value: "2 files · +18 −4", effect: "none" },
    { icon: "coverage", label: "Coverage", value: "84% of touched lines", effect: "lowers" },
    { icon: "area-mode", label: "Area mode", value: "Area auth is on Full: 1 revert in 14 days · 8 merges", effect: "none" },
  ],
  outcome: {
    heading: "Merges by itself · top of your queue",
    text: "Signing in to another engine lets Crew re-run the Security review there, and the tier can go back down.",
  },
};

export const demotedMedium: RiskReport = {
  tier: "medium",
  title: "Add a rate limit to the login route",
  reasons: [
    { icon: "paths", label: "Paths", value: "auth/redirect.ts (an auth path)", effect: "raises" },
    { icon: "size", label: "Size", value: "2 files · +18 −4", effect: "none" },
    { icon: "coverage", label: "Coverage", value: "84% of touched lines", effect: "lowers" },
    { icon: "revert-rate", label: "Revert rate", value: "auth/ · 2 of 14 · last 60 days", effect: "raises" },
    { icon: "area-mode", label: "Area mode", value: "Area auth is on On: 2 reverts in 14 days · 9 merges", effect: "raises" },
  ],
  outcome: {
    heading: "Merges by itself · review within 24 hours",
    text: "auth/ is on On after 2 reverts in 14 days, so a High risk change in auth/ now waits for you before it merges.",
  },
};

export const demotedLow: RiskReport = {
  tier: "low",
  title: "Update the login docs",
  reasons: [
    { icon: "paths", label: "Paths", value: "docs/login.md", effect: "lowers" },
    { icon: "size", label: "Size", value: "1 file · +6 −2", effect: "lowers" },
    { icon: "area-mode", label: "Area mode", value: "Area docs is on Full: 0 reverts in 14 days · 12 merges", effect: "none" },
  ],
  outcome: { heading: "Merges by itself", text: "docs/ follows your ceiling, Full. Review it in a batch whenever you like." },
};

/** The popover's rows are shorter than the card's: each reason and how it moved the tier. */
export const popoverRows: { label: string; value: string }[] = [
  { label: "Paths", value: "auth/redirect.ts: auth paths count as higher" },
  { label: "Size", value: "2 files · +18 −4: small" },
  { label: "Coverage", value: "84% of touched lines: good" },
  { label: "Revert rate", value: "auth/ 2 of 14 · Sonnet 5.5 in auth/ 1 of 9" },
  { label: "Area mode", value: "Area auth is on Full: 1 revert in 14 days · 8 merges" },
];
