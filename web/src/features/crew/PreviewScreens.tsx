import type { ComponentType } from "react";
import ReviewPreviewScreens from "./autopilot/review/PreviewScreens";
import { REVIEW_FRAMES, type ReviewFrameId } from "./autopilot/review/names";
import { CAPACITY_SCREENS } from "./capacity/screens";
import { ESCAPE_SCREENS } from "./escape/screens";
import { SETTINGS_SCREENS } from "./settings/screens";
import "./shared/frame.css";

const DRAWN: Record<string, ComponentType> = { ...ESCAPE_SCREENS, ...CAPACITY_SCREENS, ...SETTINGS_SCREENS };

const isReview = (name: string): name is ReviewFrameId => (REVIEW_FRAMES as readonly string[]).includes(name);

/** Loaded only through `crew/index.ts`, so a production build never includes it. */
export default function CrewPreviewScreens({ name }: { name: string }) {
  if (isReview(name)) return <ReviewPreviewScreens name={name} />;
  const Screen = DRAWN[name];
  return Screen ? (
    <main className="cx-root">
      <Screen />
    </main>
  ) : null;
}
