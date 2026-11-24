//! The person's plain views of crew lessons (KC5 slice 3): one lesson with whether the next brief would carry it, the
//! list with its switch and figures, and one lesson's detail with its evidence ids. These are core types for the routes
//! to shape; the wire contract types arrive with the routes. A lesson is data in a brief, never a rule, and nothing here
//! routes, verifies, holds a lease, reads an answer key or changes a check.

use crate::store::{CrewLesson, LessonEvidence};

/// Which lessons a list asks for. `Hidden` lists the over-cap ones, which stay readable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LessonFilter {
    Active,
    Disabled,
    Hidden,
    All,
}

impl LessonFilter {
    /// Whether one stored lesson belongs in this filter. A hidden lesson is only under `Hidden`, whichever state it has.
    pub(crate) fn keeps(self, lesson: &CrewLesson) -> bool {
        match self {
            Self::Active => lesson.state == "active" && !lesson.hidden_by_cap,
            Self::Disabled => lesson.state == "disabled" && !lesson.hidden_by_cap,
            Self::Hidden => lesson.hidden_by_cap,
            Self::All => true,
        }
    }
}

/// One lesson as a card shows it, with the same answer `in_next_brief` the contract gives. `in_next_brief` is false
/// while the lesson is a source of an approved proposal whose skill still exists, and `skill` names it then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LessonView {
    pub lesson: CrewLesson,
    pub in_next_brief: bool,
    /// The skill this lesson is now, while that skill exists; none when it is an ordinary lesson.
    pub skill: Option<String>,
}

/// One repository's lesson list: its switch, the brief's caps, the counts and the lessons (repository ones then global,
/// newest first, under a hidden filter only the over-cap ones).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LessonListView {
    pub repo: String,
    pub use_lessons: bool,
    pub brief_cap_lessons: usize,
    pub brief_cap_chars: usize,
    /// `(active, disabled, hidden)` for this repository's lessons plus the global ones.
    pub counts: (u32, u32, u32),
    pub lessons: Vec<LessonView>,
}

/// One lesson with the ids it came from, never their content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LessonDetailView {
    pub lesson: LessonView,
    pub evidence: Vec<LessonEvidence>,
}
