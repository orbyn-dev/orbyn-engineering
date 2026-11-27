//! A person's writes on crew lessons (KC5 slice 3, store and core): list, detail, add by hand, edit, on/off, promote
//! and demote, the 10 s delete and undo, and the per-repository switch. Every public method is behind Crew Preview, and
//! every write goes through the store's one immediate transaction, which audits first. The contract's wire types and
//! routes arrive with part 2; these methods answer plain core values.
//!
//! A lesson is data in a brief, never a rule: nothing here can touch the router, the verifier, an answer key, a lease, a
//! budget or a check. A person's words take the same cleaning, secret scan and path rule as every other lesson; the
//! `in_next_brief` flag is the brief's own `choose_lessons` over this device's active lessons, never a second rule.

use std::collections::HashSet;

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{CrewLesson, UNDO_SECS};
use crate::util::{format_unix, parse_time};

use super::lessons::{
    BRIEF_CHARS_MAX, BRIEF_LESSONS_MAX, LessonScope, LessonSkills, choose_lessons, names_path,
    person_text,
};
use super::lessons_view::{LessonDetailView, LessonFilter, LessonListView, LessonView};

fn lesson_missing() -> CoreError {
    CoreError::NotFound("lesson".into())
}

/// The ids the next brief would carry, from this repository's switch and the same chooser the brief uses. No key
/// needles: the card says whether the lesson is brief-eligible; the left-out count is separate.
fn chosen_ids(
    use_lessons: bool,
    repo: &[CrewLesson],
    global: &[CrewLesson],
    skills: &LessonSkills,
) -> HashSet<String> {
    if !use_lessons {
        return HashSet::new();
    }
    let (chosen, _) = choose_lessons(repo, global, &[], skills);
    chosen.into_iter().map(|lesson| lesson.id.clone()).collect()
}

impl Core {
    /// One repository's lessons: its own then the global ones, newest first, under the filter. Unknown repository is
    /// not found. `in_next_brief` is the brief's own choice for this repository.
    pub fn crew_lessons_list(&self, repo: &str, filter: LessonFilter) -> Result<LessonListView> {
        self.crew_preview_only()?;
        self.store.project(repo)?;
        let use_lessons = self.store.crew_use_lessons(repo)?;
        let counts = self.store.crew_lessons_counts(repo)?;
        let repo_lessons = self.store.crew_lessons("repo", Some(repo))?;
        let global_lessons = self.store.crew_lessons("global", None)?;
        let skills = self.lesson_skill_names(repo)?;
        let chosen = chosen_ids(use_lessons, &repo_lessons, &global_lessons, &skills);
        let lessons = repo_lessons
            .into_iter()
            .chain(global_lessons)
            .filter(|lesson| filter.keeps(lesson))
            .map(|lesson| LessonView {
                in_next_brief: chosen.contains(&lesson.id),
                skill: skills.get(&lesson.id).cloned(),
                lesson,
            })
            .collect();
        Ok(LessonListView {
            repo: repo.to_owned(),
            use_lessons,
            brief_cap_lessons: BRIEF_LESSONS_MAX,
            brief_cap_chars: BRIEF_CHARS_MAX,
            counts,
            lessons,
        })
    }

    /// One lesson with the ids it came from. A repository lesson ignores `repo`; a global lesson's `in_next_brief`
    /// answers for `repo`, and is false when no repository is named.
    pub fn crew_lesson_detail(&self, id: &str, repo: Option<&str>) -> Result<LessonDetailView> {
        self.crew_preview_only()?;
        let lesson = self.store.crew_lesson(id)?.ok_or_else(lesson_missing)?;
        let project = if lesson.scope == "global" {
            match repo {
                Some(repo) => {
                    self.store.project(repo)?;
                    Some(repo.to_owned())
                }
                None => None,
            }
        } else {
            lesson.project_id.clone()
        };
        let (in_next_brief, skill) = match project.as_deref() {
            Some(project) => {
                let use_lessons = self.store.crew_use_lessons(project)?;
                let skills = self.lesson_skill_names(project)?;
                let in_next_brief = if use_lessons {
                    let repo_lessons = self.store.crew_lessons("repo", Some(project))?;
                    let global_lessons = self.store.crew_lessons("global", None)?;
                    chosen_ids(true, &repo_lessons, &global_lessons, &skills).contains(&lesson.id)
                } else {
                    false
                };
                (in_next_brief, skills.get(&lesson.id).cloned())
            }
            None => (false, None),
        };
        let evidence = lesson.evidence.clone();
        Ok(LessonDetailView {
            lesson: LessonView {
                lesson,
                in_next_brief,
                skill,
            },
            evidence,
        })
    }

    /// Adds one lesson by hand: the person's own words, source `person`, the same secret scan and path rule as every
    /// correction. An unknown repository is not found.
    pub fn crew_lesson_add(&self, scope: LessonScope, text: &str) -> Result<CrewLesson> {
        self.crew_preview_only()?;
        if let Some(project) = scope.project() {
            self.store.project(project)?;
        }
        self.crew_lesson_from_person_correction(scope, text, Vec::new())
    }

    /// Edits one lesson: cleaned and secret-scanned as a person's words, `edited_by_person` set, `names_path`
    /// recomputed. A global lesson that now names a path is refused, exactly as adding one is.
    pub fn crew_lesson_edit(&self, id: &str, text: &str, seen_rev: u32) -> Result<CrewLesson> {
        self.crew_preview_only()?;
        let current = self.store.crew_lesson(id)?.ok_or_else(lesson_missing)?;
        let text = person_text(text)?;
        let names_path = names_path(&text);
        if current.scope == "global" && names_path {
            return Err(CoreError::Invalid(
                "A global lesson names path classes, not paths.".into(),
            ));
        }
        self.store.edit_crew_lesson(id, &text, names_path, seen_rev)
    }

    /// Turns one lesson on or off. The store refuses a stale `seen_rev`.
    pub fn crew_lesson_set_enabled(
        &self,
        id: &str,
        enabled: bool,
        seen_rev: u32,
    ) -> Result<CrewLesson> {
        self.crew_preview_only()?;
        self.store.set_crew_lesson_state(id, enabled, seen_rev)
    }

    /// Moves a repository lesson to global; the store refuses one that names a path and hides an over-cap global one.
    pub fn crew_lesson_promote(&self, id: &str, seen_rev: u32) -> Result<CrewLesson> {
        self.crew_preview_only()?;
        self.store.promote_crew_lesson(id, seen_rev)
    }

    /// Moves a global lesson back to the repository the person chose; an unknown repository is not found.
    pub fn crew_lesson_demote(&self, id: &str, repo: &str, seen_rev: u32) -> Result<CrewLesson> {
        self.crew_preview_only()?;
        self.store.project(repo)?;
        self.store.demote_crew_lesson(id, repo, seen_rev)
    }

    /// Deletes one lesson for the 10 s Undo and answers its id and the instant the Undo ends.
    pub fn crew_lesson_delete(&self, id: &str, seen_rev: u32) -> Result<(String, String)> {
        self.crew_preview_only()?;
        let deleted_at = self.store.delete_crew_lesson(id, seen_rev)?;
        let undo_until = parse_time(&deleted_at)
            .map(|at| format_unix(at.saturating_add(UNDO_SECS)))
            .ok_or_else(|| {
                CoreError::Internal("a deleted lesson's time could not be read".into())
            })?;
        Ok((id.to_owned(), undo_until))
    }

    /// Restores one lesson while its 10 s Undo is open; after that, or for an unknown id, it is not found.
    pub fn crew_lesson_undo(&self, id: &str) -> Result<CrewLesson> {
        self.crew_preview_only()?;
        self.store.undo_delete_crew_lesson(id)
    }

    /// Sets one repository's "use lessons" switch. Unknown repository is not found.
    pub fn crew_lessons_set_use(&self, repo: &str, on: bool) -> Result<()> {
        self.crew_preview_only()?;
        self.store.project(repo)?;
        self.store.set_crew_use_lessons(repo, on, "user")
    }
}
