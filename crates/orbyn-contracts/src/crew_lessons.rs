//! Crew lessons over HTTP (Crew Preview, KC5 slice 3; docs/crew-lessons.md): the short sentences the crew keeps about one
//! repository or all of them, with where each came from. A lesson is data in a brief, never a rule: no field here can touch
//! the router, the verifier, an answer key, a lease, a budget or a check, and nothing here is synced or exported. Ids,
//! enums, one short text and counts only; a lesson's fingerprint and deleted_at stay in the store and never reach a client.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// What a lesson is about: one repository (its project id) or every repository on this device.
pub enum LessonScope {
    Repo,
    Global,
}
}

wire_enum! {
/// Which lessons a list asks for. `Hidden` is the over-cap ones, which stay readable; `All` is every lesson of the
/// repository plus the global ones.
pub enum LessonFilter {
    Active,
    Disabled,
    Hidden,
    All,
}
}

wire! {
/// One lesson as a card shows it.
pub struct Lesson {
    pub id: String,
    pub scope: LessonScope,
    /// The project id of a repository lesson; null when the scope is global.
    pub repo: Option<String>,
    /// `fingerprint`, `escape`, `revert` or `person`.
    pub source: String,
    /// 1 to 300 characters.
    pub text: String,
    /// `active` or `disabled`.
    pub state: String,
    /// Over the scope's cap: readable under the Hidden filter, never deleted for it.
    pub hidden_by_cap: bool,
    /// The text names a repository-relative path; promoting such a lesson is refused until it is edited.
    pub names_path: bool,
    pub edited_by_person: bool,
    /// Briefs that carried it.
    pub uses: u32,
    /// Briefs it was left out of because their answer-key check failed.
    pub left_out_of: u32,
    /// The skill this lesson is now, while that skill exists; null for an ordinary lesson.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub skill: Option<String>,
    /// The ids the lesson came from, at most five; ids only, never content.
    pub evidence: Vec<LessonEvidence>,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
    /// The version a write sends back; a different one answers `stale`.
    pub rev: u32,
    /// Whether the next brief for a repository would carry it: active, inside the brief's caps, the switch on and no skill.
    /// A global lesson answers for the `repo` named on its read, and false without one.
    pub in_next_brief: bool,
}
}

wire! {
/// One id a lesson came from, never its content.
pub struct LessonEvidence {
    /// `task`, `escape` or `merge`.
    pub kind: String,
    pub id: String,
}
}

wire! {
/// The most a brief carries: five lessons and 1500 characters of their text.
pub struct LessonBriefCap {
    pub lessons: u32,
    pub chars: u32,
}
}

wire! {
/// One repository's figures: its own lessons plus the global ones. A hidden lesson is counted there and in no state.
pub struct LessonCounts {
    pub active: u32,
    pub disabled: u32,
    pub hidden: u32,
}
}

wire! {
/// One repository's lesson list: its switch, the brief's caps, the counts and the lessons (repository ones then global,
/// newest first, under the filter).
pub struct LessonList {
    pub repo: String,
    pub use_lessons: bool,
    pub brief_cap: LessonBriefCap,
    pub counts: LessonCounts,
    pub lessons: Vec<Lesson>,
}
}

wire! {
/// One lesson with the ids it came from.
pub struct LessonDetail {
    pub lesson: Lesson,
    pub evidence: Vec<LessonEvidence>,
}
}

wire! {
/// A person's new lesson by hand (`POST /api/crew/lessons`). `repo` is required for scope `repo` and absent for `global`.
/// Any other field is refused.
#[serde(deny_unknown_fields)]
pub struct LessonAdd {
    pub scope: LessonScope,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub repo: Option<String>,
    pub text: String,
}
}

wire! {
/// A person's edit of one lesson (`PATCH /api/crew/lessons/{id}`). `rev` is the version the screen showed. Any other field
/// is refused.
#[serde(deny_unknown_fields)]
pub struct LessonEdit {
    pub text: String,
    pub rev: u32,
}
}

wire! {
/// The version a person saw, for one of the one-answer writes (`disable`, `enable`, `promote`). Any other field is refused.
#[serde(deny_unknown_fields)]
pub struct LessonRev {
    pub rev: u32,
}
}

wire! {
/// A person moves a global lesson back to one repository (`POST /api/crew/lessons/{id}/demote`). Any other field is refused.
#[serde(deny_unknown_fields)]
pub struct LessonDemote {
    pub repo: String,
    pub rev: u32,
}
}

wire! {
/// A person's "use lessons" switch for one repository (`PUT /api/crew/lessons/settings`). Any other field is refused.
#[serde(deny_unknown_fields)]
pub struct LessonSettings {
    pub repo: String,
    pub use_lessons: bool,
}
}

wire! {
/// The answer to a delete: the lesson's id and the instant its 10 s Undo ends, as RFC 3339.
pub struct LessonDeleted {
    pub id: String,
    pub undo_until: String,
}
}

wire! {
/// One repository's "use lessons" switch after a person saved it.
pub struct LessonSettingsView {
    pub repo: String,
    pub use_lessons: bool,
}
}
