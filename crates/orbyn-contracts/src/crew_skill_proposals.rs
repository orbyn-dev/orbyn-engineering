//! Crew skill proposals over HTTP (Crew Preview, KC9; docs/crew-lessons.md): the fixed-template skill a person may approve
//! when three or more active lessons of one repository share a group key. A proposal is data, never a rule: no field here
//! can touch the router, the verifier, an answer key, a lease, a budget or a check, and a proposal blocks nothing. Ids,
//! one short text and evidence ids only; the skill's text and where it is written stay in the core.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::crew_lessons::LessonEvidence;

wire_enum! {
/// Where one proposal stands. An approved or dismissed proposal stays readable so the screen can show what happened.
pub enum SkillProposalState {
    Open,
    Approved,
    Dismissed,
}
}

wire! {
/// The local skill an approved proposal wrote, by name.
pub struct SkillProposalSkill {
    pub name: String,
}
}

wire! {
/// One source lesson of a proposal: its id and the evidence ids it holds now. A lesson deleted for good answers no
/// evidence; its id stays in the proposal.
pub struct SkillProposalLesson {
    pub id: String,
    pub evidence: Vec<LessonEvidence>,
}
}

wire! {
/// One skill proposal as a person's screen reads it.
pub struct SkillProposal {
    pub id: String,
    /// The project id whose lessons proposed it.
    pub repo: String,
    pub state: SkillProposalState,
    /// The template text; a person may edit it before Approve.
    pub text: String,
    /// The source lessons, with their own current evidence.
    pub lessons: Vec<SkillProposalLesson>,
    /// Whether every source lesson is still active and visible, so it can be approved now.
    pub can_approve: bool,
    /// The skill an approval wrote; null while the proposal is open or dismissed.
    pub skill: Option<SkillProposalSkill>,
    /// The version a write sends back; a different one answers `stale`.
    pub rev: u32,
    pub created_at: String,
    /// When it was approved, RFC 3339; null before.
    pub approved_at: Option<String>,
}
}

wire! {
/// One repository's proposals, every state, newest first.
pub struct SkillProposalList {
    pub proposals: Vec<SkillProposal>,
}
}

wire! {
/// A person approves one open proposal (`POST /api/crew/skill-proposals/{id}/approve`). `rev` is the version the screen
/// showed. Any other field is refused.
#[serde(deny_unknown_fields)]
pub struct SkillProposalApprove {
    pub text: String,
    pub rev: u32,
}
}

wire! {
/// A person dismisses one open proposal (`POST /api/crew/skill-proposals/{id}/dismiss`). Any other field is refused.
#[serde(deny_unknown_fields)]
pub struct SkillProposalDismiss {
    pub rev: u32,
}
}
