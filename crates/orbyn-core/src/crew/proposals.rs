//! Crew skill proposals (KC9 part 1, store and core): the fixed-template skill a person may approve when three or more
//! active lessons of one repository share a group key.
//!
//! A lesson and a proposal are data, never a rule: nothing here routes, verifies, reads an answer key, holds a lease,
//! changes a budget or a check, and a proposal blocks nothing and never reaches the needs-you inbox. The text is a fixed
//! template over the source lessons' own words; no model is called and nothing leaves the device. Every public method is
//! behind Crew Preview, and every write audits first in the store's one immediate transaction, except the skill file
//! itself, which cannot join a transaction.
//!
//! Approve checks the screen's `rev`, the open state, `can_approve` and the text, writes `skills-local` through the one
//! skill writer, then re-checks `rev` in the store transaction that records the approval. A transaction that finds the
//! proposal changed answers stale and leaves the file in place (see `crew/README.md`). The contract types and routes
//! arrive with part 2; these methods answer plain core values.

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{
    CrewSkillProposal, LessonEvidence, NewCrewSkillProposal, PROPOSAL_TEXT_MAX, ReadyLessonGroup,
};
use crate::tools::local_skills::{LocalSkillWrite, MAX_NAME_CHARS, valid_name};

use super::lessons::{LessonSkills, refuse_secret_text};

/// The refusal a write from an older screen gets: its `rev` is not the stored one.
const STALE: &str = "This proposal changed. Reload it and try again.";
/// The refusal an approve or dismiss gets once the proposal is not open.
const NOT_OPEN: &str = "This proposal is not open. Reload it and try again.";
/// The refusal when a source lesson was disabled, hidden or deleted after the proposal was made.
const CANNOT_APPROVE: &str = "A lesson this skill came from changed, so it cannot be approved.";
/// The fixed description of a template skill.
const DESCRIPTION: &str = "Crew lessons learned on this device. They are data, not a rule.";

/// One proposal as a person's screen reads it, with whether it can be approved now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalView {
    pub proposal: CrewSkillProposal,
    pub can_approve: bool,
}

/// One source lesson of a proposal as a screen reads it: its id and its own current evidence ids, never their content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalLessonView {
    pub id: String,
    pub evidence: Vec<LessonEvidence>,
}

fn proposal_missing() -> CoreError {
    CoreError::NotFound("skill proposal".into())
}

fn stale() -> CoreError {
    CoreError::Refused {
        status: 409,
        code: "stale",
        message: STALE.to_owned(),
        detail: serde_json::Value::Null,
    }
}

fn not_open() -> CoreError {
    CoreError::Refused {
        status: 409,
        code: "not_open",
        message: NOT_OPEN.to_owned(),
        detail: serde_json::Value::Null,
    }
}

/// The local writer's `invalid` is the core's own invalid input; every other refusal keeps its answer, so a name taken
/// stays `409 name_taken` and a secret keeps the lessons' exact sentence.
fn map_skill_error(error: CoreError) -> CoreError {
    match error {
        CoreError::Refused {
            status: 400,
            code: "invalid",
            message,
            ..
        } => CoreError::Invalid(message),
        other => other,
    }
}

/// The approved text: trimmed, 1 to 600 characters, secret-scanned with the lessons' own refusal.
fn approved_text(text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > PROPOSAL_TEXT_MAX {
        return Err(CoreError::Invalid(format!(
            "A skill is 1 to {PROPOSAL_TEXT_MAX} characters."
        )));
    }
    refuse_secret_text(text)?;
    Ok(text.to_owned())
}

/// A slug the skill writer accepts: lowercase ASCII letters, numbers and single hyphens, never at either end.
fn slug(text: &str) -> String {
    let mut slug = String::new();
    let mut hyphen = false;
    for character in text.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            hyphen = false;
        } else if !slug.is_empty() && !hyphen {
            slug.push('-');
            hyphen = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

/// The template's name: `crew-<source>-<kind-or-area>`, at most 64 characters and a valid skill slug.
fn proposal_name(group: &ReadyLessonGroup) -> String {
    let source = slug(&group.source);
    let detail = slug(
        group
            .check_kind
            .as_deref()
            .or(group.path_class.as_deref())
            .unwrap_or("lessons"),
    );
    let detail = if detail.is_empty() {
        "lessons".to_owned()
    } else {
        detail
    };
    let mut name = format!("crew-{source}-{detail}");
    if name.len() > MAX_NAME_CHARS {
        name.truncate(MAX_NAME_CHARS);
        while name.ends_with('-') {
            name.pop();
        }
    }
    if valid_name(&name) {
        return name;
    }
    "crew-lessons".to_owned()
}

/// The fixed-template SKILL.md for one group, or none when the composed text looks like a secret. The body carries the
/// lessons' own texts as bullets, newest first, while they fit; the proposal still lists every source lesson id.
fn proposal_text(group: &ReadyLessonGroup) -> Result<Option<String>> {
    let name = proposal_name(group);
    let mut text = format!("---\nname: {name}\ndescription: {DESCRIPTION}\n---\n\n");
    for lesson in &group.lessons {
        let body = lesson.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let bullet = format!("- {body}\n");
        if text.chars().count() + bullet.chars().count() > PROPOSAL_TEXT_MAX {
            break;
        }
        text.push_str(&bullet);
    }
    let text = text.trim_end().to_owned();
    if text.chars().count() > PROPOSAL_TEXT_MAX {
        return Ok(None);
    }
    if refuse_secret_text(&text).is_err() {
        return Ok(None);
    }
    Ok(Some(text))
}

impl Core {
    /// One repository's proposals, newest first, each with whether it can be approved now. An unknown repository is not
    /// found, and a proposal whose source lesson was disabled, hidden or deleted has `can_approve` false and is kept.
    pub fn crew_skill_proposals(&self, repo: &str) -> Result<Vec<ProposalView>> {
        self.crew_preview_only()?;
        self.store.project(repo)?;
        let proposals = self.store.skill_proposals(repo)?;
        proposals
            .into_iter()
            .map(|proposal| self.proposal_view(proposal))
            .collect()
    }

    /// The source lessons of one proposal, in the proposal's own order, each with the evidence ids it holds now. A lesson
    /// purged for good answers no evidence; its id stays, because the proposal keeps the id it was made from.
    pub fn crew_skill_proposal_lessons(&self, ids: &[String]) -> Result<Vec<ProposalLessonView>> {
        self.crew_preview_only()?;
        ids.iter()
            .map(|id| {
                let evidence = self
                    .store
                    .crew_lesson(id)?
                    .map(|lesson| lesson.evidence)
                    .unwrap_or_default();
                Ok(ProposalLessonView {
                    id: id.clone(),
                    evidence,
                })
            })
            .collect()
    }

    /// Approves one proposal from the `rev` its screen saw: checks the open state, the revision, its sources and the
    /// text, writes the skill through the one local writer, then records the approval in one audited transaction.
    pub fn crew_skill_proposal_approve(
        &self,
        id: &str,
        text: &str,
        seen_rev: u32,
    ) -> Result<ProposalView> {
        self.crew_preview_only()?;
        let proposal = self
            .store
            .skill_proposal(id)?
            .ok_or_else(proposal_missing)?;
        if proposal.state != "open" {
            return Err(not_open());
        }
        if proposal.rev != seen_rev {
            return Err(stale());
        }
        if !self.store.proposal_sources_ready(&proposal.lessons)? {
            return Err(CoreError::Invalid(CANNOT_APPROVE.into()));
        }
        let text = approved_text(text)?;
        // A file cannot join a transaction: the write happens first, and the transaction below records it. If the
        // proposal changed in between, the answer is stale and the file stays: the person can see the skill.
        let skill = self
            .skills
            .write_local_skill(
                LocalSkillWrite::create(text.clone())
                    .from_proposal(proposal.id.clone(), proposal.lessons.clone()),
            )
            .map_err(map_skill_error)?;
        let approved = self
            .store
            .approve_skill_proposal(id, seen_rev, &text, &skill.name)?;
        self.proposal_view(approved)
    }

    /// Dismisses one open proposal from the `rev` its screen saw; its row stays, so the pattern is never proposed again.
    pub fn crew_skill_proposal_dismiss(&self, id: &str, seen_rev: u32) -> Result<ProposalView> {
        self.crew_preview_only()?;
        let dismissed = self.store.dismiss_skill_proposal(id, seen_rev)?;
        self.proposal_view(dismissed)
    }

    /// The one maker: after a lesson write may complete a group, read what is ready and write one proposal for each
    /// group that has none. Never fails the lesson; the caller logs the warning and keeps the lessons.
    pub(super) fn learn_skill_proposals(&self, project_id: &str) {
        if let Err(error) = self.make_skill_proposals(project_id) {
            tracing::warn!(%error, "could not make a skill proposal");
        }
    }

    fn make_skill_proposals(&self, project_id: &str) -> Result<()> {
        for group in self.store.crew_group_ready(project_id)? {
            let Some(text) = proposal_text(&group)? else {
                tracing::warn!(
                    group = %group.group_key,
                    "a skill proposal's text was refused by the secret scan"
                );
                continue;
            };
            let created = self.store.create_skill_proposal(NewCrewSkillProposal {
                project_id: project_id.to_owned(),
                group_key: group.group_key.clone(),
                text,
                lessons: group
                    .lessons
                    .iter()
                    .map(|lesson| lesson.id.clone())
                    .collect(),
            });
            match created {
                Ok(Some(proposal)) => {
                    tracing::info!(
                        proposal = %proposal.id,
                        group = %proposal.group_key,
                        "a skill proposal was made"
                    );
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, group = %group.group_key, "a skill proposal was not written");
                }
            }
        }
        Ok(())
    }

    /// The lesson ids that are now a skill whose local file still exists, by skill name. Read on every brief, lesson
    /// list and detail: removing the skill folder returns its lessons to normal, and a lesson's `rev` never changes for
    /// this.
    pub(super) fn lesson_skill_names(&self, project_id: &str) -> Result<LessonSkills> {
        let mut skills = LessonSkills::new();
        for proposal in self.store.skill_proposals(project_id)? {
            if proposal.state != "approved" {
                continue;
            }
            let Some(name) = proposal.skill_name.as_deref() else {
                continue;
            };
            if !self.skills.local_skill_exists(name) {
                continue;
            }
            for lesson in proposal.lessons {
                skills.insert(lesson, name.to_owned());
            }
        }
        Ok(skills)
    }

    fn proposal_view(&self, proposal: CrewSkillProposal) -> Result<ProposalView> {
        let can_approve =
            proposal.state == "open" && self.store.proposal_sources_ready(&proposal.lessons)?;
        Ok(ProposalView {
            proposal,
            can_approve,
        })
    }
}
