/**
 * The lessons types (KC5, KC9). The wire shapes are the contract's own, generated into web/src/generated: a lesson carries
 * the skill it became as its name (`skill`), and the evidence ids it came from (`evidence`). The card, the list and the
 * proposals read those fields directly, so this file only re-exports them beside the local source vocabulary.
 */

export type { Lesson } from "../../../generated/Lesson";
export type { LessonDetail } from "../../../generated/LessonDetail";
export type { LessonList } from "../../../generated/LessonList";

export type { LessonAdd } from "../../../generated/LessonAdd";
export type { LessonBriefCap } from "../../../generated/LessonBriefCap";
export type { LessonCounts } from "../../../generated/LessonCounts";
export type { LessonDeleted } from "../../../generated/LessonDeleted";
export type { LessonDemote } from "../../../generated/LessonDemote";
export type { LessonEdit } from "../../../generated/LessonEdit";
export type { LessonEvidence } from "../../../generated/LessonEvidence";
export type { LessonFilter } from "../../../generated/LessonFilter";
export type { LessonRev } from "../../../generated/LessonRev";
export type { LessonScope } from "../../../generated/LessonScope";
export type { LessonSettings } from "../../../generated/LessonSettings";
export type { LessonSettingsView } from "../../../generated/LessonSettingsView";

/** The known sources, used only as the keys of the card's word tables; a read may carry any string. */
export type LessonSource = "fingerprint" | "escape" | "revert" | "person";
