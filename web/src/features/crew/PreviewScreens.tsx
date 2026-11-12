import { lazy, Suspense } from "react";
import ReviewPreviewScreens from "./autopilot/review/PreviewScreens";
import { REVIEW_FRAMES, type ReviewFrameId } from "./autopilot/review/names";

const DrawnScreens = lazy(() => import("./DrawnScreens"));

const isReview = (name: string): name is ReviewFrameId => (REVIEW_FRAMES as readonly string[]).includes(name);

/** Loaded only through `crew/index.ts`, so a production build never includes it. */
export default function CrewPreviewScreens({ name }: { name: string }) {
  if (isReview(name)) return <ReviewPreviewScreens name={name} />;
  return (
    <Suspense fallback={null}>
      <DrawnScreens name={name} />
    </Suspense>
  );
}
