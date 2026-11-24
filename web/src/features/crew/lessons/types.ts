/**
 * The lessons types (KC5, KC9). The wire shapes are the contract's own, generated into web/src/generated. The one local
 * addition is `skill`, which the KC9 proposals PR adds to the generated Lesson: the card and the list already read it
 * while a read carries it, so it stays optional here until then.
 */

import type { Lesson as GeneratedLesson } from "../../../generated/Lesson";
import type { LessonDetail as GeneratedLessonDetail } from "../../../generated/LessonDetail";
import type { LessonList as GeneratedLessonList } from "../../../generated/LessonList";

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

/** A lesson as the service answers it, plus the skill the KC9 proposals slice carries beside it. */
export type Lesson = GeneratedLesson & { skill?: { name: string } | null };

/** One repository's list: the generated list, with its lessons carrying the optional `skill`. */
export type LessonList = Omit<GeneratedLessonList, "lessons"> & { lessons: Lesson[] };

/** One lesson and its evidence: the generated detail, with its lesson carrying the optional `skill`. */
export type LessonDetail = Omit<GeneratedLessonDetail, "lesson"> & { lesson: Lesson };
