import { lazy } from "react";

export { SnapshotRuntime } from "./SnapshotRuntime";
export { SnapshotSettingsSection } from "./SnapshotSettingsSection";
export { SnapshotDraftChip, SnapshotCapturingChip } from "./SnapshotChip";
export { SnapshotButton } from "./SnapshotButton";
export { useSnapshotDraftFlow, type SnapshotDraftFlow } from "./useSnapshotDraftFlow";
export { useSnapshot } from "./runtime";
export type { SnapShotAttachment, SnapShotEventPayload, SnapShotState, SnapShotSource, SnapShotDraft, SnapShotSettings, SnapShotShortcut, SnapShotTarget } from "./types";
export {
  ACCESSIBILITY_TEXT_WARNING,
  BUILT_IN_EXCLUDED_APPS,
  MAX_EXCLUDED_APPS,
  MAX_EXCLUDED_APP_LENGTH,
  SNAPSHOT_EVENTS,
  SNAPSHOT_FRAMES,
  isSnapshotFrame,
  excludedAppProblem,
  fileOfTake,
  isSnapShotSource,
  shortcutHint,
  shortcutKeys,
  shortcutText,
  snapshotSettingsOf,
  withoutAccessibleText,
  sourceBodyOf,
  sourceBodyToSend,
} from "./types";

/** Dev-only: `#/preview/snapshot?frame=<id>`. The production flag is false, so the lazy chunk is never emitted. */
export const SnapshotPreview = __ORBYN_PREVIEWS__ ? lazy(() => import("./SnapshotPreview")) : null;
export const snapshotPreviewRoute = (hash: string): boolean => __ORBYN_PREVIEWS__ && hash.startsWith("#/preview/snapshot");
