import "./review.css";
import { useState, type ReactNode } from "react";
import { BatchScreen } from "./Batch";
import { DigestScreen } from "./Digest";
import { ForecastScreen, MergeQueueEmptyScreen, MergeQueueScreen } from "./MergeQueue";
import { CrewPanel } from "./Panel";
import { ReviewItemScreen } from "./ReviewItem";
import { ReviewQueueScreen, queueCounts } from "./ReviewQueue";
import {
  RiskBoardScreen,
  RiskDemotedScreen,
  RiskHighFullScreen,
  RiskHighOnScreen,
  RiskPopoverScreen,
  RiskRaisedScreen,
  RiskReasonsScreen,
} from "./Risk";
import type { ReviewFrameId } from "./names";
import type { QueueData } from "./model";
import { watchAsks, watchBisecting, watchBroke, watchFlaky, watchReverted, digestDaily, digestQuiet } from "./sampleWatch";
import { forecast, mergeItem, mergeItemWithDependents, mergeQueue, mergeQueueRemoved } from "./sampleMerge";
import {
  batch,
  queue,
  queueDemoted,
  queueEmpty,
  queueOverdue,
  queueOverdueHigh,
  queuePromoted,
  queueReverted,
} from "./sampleQueue";
import { WatchScreen } from "./Watch";
import { FlowAfterMergeScreen } from "./FlowAfterMerge";

const go = (target: string) => {
  window.location.hash = `#/preview/crew/${target}`;
};

function queueScreen(data: QueueData) {
  return (
    <ReviewQueueScreen
      data={data}
      onOpen={(_entry, dialog) => go(dialog === "comment" ? "review-comment" : dialog === "revert" ? "review-revert" : "review-item")}
      onDigest={() => go("digest-daily")}
    />
  );
}

function PanelFrame({ initial }: { initial: "review" | "merge" }) {
  const [view, setView] = useState(initial);
  return (
    <div className="crew-panel-stage">
      <CrewPanel view={view} onView={setView} onOpenInCrew={() => go(view === "review" ? "review-queue" : "mergequeue-main")} />
    </div>
  );
}

const itemScreen = (item: typeof mergeItem, dialog: "comment" | "revert" | null) => (
  <ReviewItemScreen item={item} initialDialog={dialog} reviewCount={queueCounts(queue)["to-review"]} onBack={() => go("review-queue")} />
);

const SCREENS: Record<ReviewFrameId, () => ReactNode> = {
  "risk-reasons": () => <RiskReasonsScreen />,
  "risk-popover": () => <RiskPopoverScreen />,
  "risk-high-full": () => <RiskHighFullScreen />,
  "risk-high-on": () => <RiskHighOnScreen />,
  "risk-raised": () => <RiskRaisedScreen />,
  "risk-board-card": () => <RiskBoardScreen />,
  "risk-demoted": () => <RiskDemotedScreen />,
  "mergequeue-main": () => <MergeQueueScreen data={mergeQueue} />,
  "mergequeue-forecast": () => <ForecastScreen data={forecast} />,
  "mergequeue-removed": () => <MergeQueueScreen data={mergeQueueRemoved} />,
  "mergequeue-empty": () => <MergeQueueEmptyScreen />,
  "mergequeue-narrow-panel": () => <PanelFrame initial="merge" />,
  "review-queue": () => queueScreen(queue),
  "review-item": () => itemScreen(mergeItem, null),
  "review-comment": () => itemScreen(mergeItem, "comment"),
  "review-revert": () => itemScreen(mergeItem, "revert"),
  "review-revert-dependents": () => itemScreen(mergeItemWithDependents, "revert"),
  "review-empty": () => queueScreen(queueEmpty),
  "review-overdue": () => queueScreen(queueOverdue),
  "review-overdue-high": () => queueScreen(queueOverdueHigh),
  "review-reverted": () => queueScreen(queueReverted),
  "review-batch": () => <BatchScreen data={batch} />,
  "review-narrow-panel": () => <PanelFrame initial="review" />,
  "review-narrow-window": () => <div className="crew-window-640">{queueScreen(queue)}</div>,
  "review-demoted": () => queueScreen(queueDemoted),
  "review-promoted": () => queueScreen(queuePromoted),
  "watch-broke": () => <WatchScreen data={watchBroke} />,
  "watch-bisecting": () => <WatchScreen data={watchBisecting} />,
  "watch-reverted": () => <WatchScreen data={watchReverted} />,
  "watch-asks": () => <WatchScreen data={watchAsks} />,
  "watch-flow": () => <FlowAfterMergeScreen />,
  "watch-flaky": () => <WatchScreen data={watchFlaky} />,
  "digest-daily": () => <DigestScreen data={digestDaily} onReview={() => go("review-queue")} />,
  "digest-empty": () => <DigestScreen data={digestQuiet} onReview={() => go("review-queue")} />,
};

/** Loaded only through `crew/index.ts`, so a production build never includes it. */
export default function ReviewPreviewScreens({ name }: { name: ReviewFrameId }) {
  return <main className="crew-root">{SCREENS[name]()}</main>;
}
