import type { ComponentType } from "react";
import { LiveEscapes } from "./LiveEscapes";

/**
 * The escape frames of Crew Designs v79, built on the real escape routes. Each opens at '#/preview/crew/<id>'; a detail
 * frame names its escape in the hash's query ('?id=<escape id>'), so the same frame draws whichever escape it is asked
 * for. A check frame opens the escape's own page too: the check is drawn in whichever state the read carries, and the
 * retry and withdraw frames are that page with their dialog opened by a click. The log's empty, low-sample, unreadable
 * and at-limit states are the log read's own answers, not separate screens.
 */
export const ESCAPE_LIVE_FRAMES = [
  "escape-log",
  "escape-empty",
  "escape-low-sample",
  "escape-error",
  "escape-possible",
  "escape-detail",
  "escape-detail-tooling",
  "escape-confirmed",
  "escape-dismissed",
  "escape-gap",
  "escape-check-proposed",
  "escape-check-writing",
  "escape-check-approve",
  "escape-check-proven",
  "escape-check-invalid",
  "escape-check-retry",
  "escape-check-withdraw",
  "escape-check-withdrawn",
  "escape-limit",
] as const;

export type EscapeLiveFrameId = (typeof ESCAPE_LIVE_FRAMES)[number];

/** The escape a detail frame names in its hash; empty when the frame is opened without one. */
export const escapeIdOfHash = (): string => new URLSearchParams(window.location.hash.split("?")[1] ?? "").get("id") ?? "";

export const ESCAPE_LIVE_SCREENS: Record<EscapeLiveFrameId, ComponentType> = {
  "escape-log": () => <LiveEscapes view="log" />,
  "escape-empty": () => <LiveEscapes view="log" />,
  "escape-low-sample": () => <LiveEscapes view="log" />,
  "escape-error": () => <LiveEscapes view="log" />,
  "escape-possible": () => <LiveEscapes view="possible" />,
  "escape-detail": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-detail-tooling": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-confirmed": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-dismissed": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-gap": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-check-proposed": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-check-writing": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-check-approve": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-check-proven": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-check-invalid": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-check-retry": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-check-withdraw": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-check-withdrawn": () => <LiveEscapes view="detail" id={escapeIdOfHash()} />,
  "escape-limit": () => <LiveEscapes view="log" />,
};
