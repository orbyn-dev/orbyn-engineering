//! Crew lessons (KC5 slice 1): the three sources the core learns from, the one sentence each writes, and the rules
//! every write keeps.
//!
//! A lesson is data in a brief, never a rule: nothing here routes, verifies, holds a lease, reads an answer key or
//! changes a check, and no write can make a task pass or fail. The sources are exactly three and none of them is
//! worker output: a **repeated failure fingerprint** (the loop guard's own hash, on two tasks of one repository
//! within 60 days), a **confirmed escape** (the escape log; a revert is its own source) and a **person's correction**
//! (only their own typed words). Text is code-owned from names the guard already redacts for the worker, or the
//! person's own words; the secret scan refuses a lesson before it is stored. The failure sentence names the
//! failing check's KIND, never its id or its command (H1), so a stored lesson cannot leak any part of the key.
//!
//! Every write goes through one path: text cleaned and capped at 300 characters, the secret scan, the path rule (a
//! global lesson names path classes, not paths), then `Store::write_crew_lesson`, which audits in the same
//! transaction and hides over-cap lessons instead of deleting them. Slice 1 stores and writes only; brief injection
//! is slice 2 and the routes are slice 3.

use orbyn_contracts::{EscapeSource, EscapeState};

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{
    CrewLesson, CrewTask, EVIDENCE_MAX, EscapeRecord, LessonEvidence, NewCrewLesson,
};
use crate::util::{parse_time, sha256_hex, unix_now};

use super::verify::{Failed, Judgement};

/// The longest lesson text, in characters.
pub const TEXT_MAX: usize = 300;
/// How many tasks of one repository must carry the same fingerprint before it becomes a lesson.
pub const THRESHOLD: usize = 2;
/// How far back the second task of a repeated failure may be.
pub const WINDOW_SECS: i64 = 60 * 24 * 60 * 60;
/// The refusal the secret scan gives, word for word (docs/crew-lessons.md).
pub const SECRET_REFUSAL: &str =
    "Not saved: this looks like it holds a secret. Edit it and save again.";
/// The escape log's own sentence for a revert; the lesson already says it was reverted.
const REVERT_DEFAULT_REASON: &str = "The merge was reverted.";

/// What a lesson is about: one repository (its project id) or every one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LessonScope {
    Repo(String),
    Global,
}

impl LessonScope {
    /// The stored word: `repo` or `global`.
    pub fn word(&self) -> &'static str {
        match self {
            Self::Repo(_) => "repo",
            Self::Global => "global",
        }
    }

    /// The project id of a repository lesson; none for a global one.
    pub fn project(&self) -> Option<&str> {
        match self {
            Self::Repo(project) => Some(project),
            Self::Global => None,
        }
    }
}

/// A lesson about to be stored, before the one write path validates it.
struct LessonDraft {
    scope: LessonScope,
    source: &'static str,
    text: String,
    fingerprint: Option<String>,
    evidence: Vec<LessonEvidence>,
}

impl Core {
    /// Records a lesson from a person's own typed words (source `person`): only their words are stored, cleaned and
    /// secret-scanned, and the lesson remembers a person wrote it. The one write path a correction uses (slice 3
    /// calls it from the Person routes).
    pub fn crew_lesson_from_person_correction(
        &self,
        scope: LessonScope,
        words: &str,
        evidence: Vec<LessonEvidence>,
    ) -> Result<CrewLesson> {
        self.crew_preview_only()?;
        let text = person_text(words)?;
        let names_path = names_path(&text);
        if names_path && scope.project().is_none() {
            return Err(CoreError::Invalid(
                "A global lesson names path classes, not paths.".into(),
            ));
        }
        self.store
            .write_crew_lesson(NewCrewLesson {
                scope: scope.word().to_owned(),
                project_id: scope.project().map(str::to_owned),
                source: "person".to_owned(),
                text,
                fingerprint: None,
                evidence,
                edited_by_person: true,
                names_path,
            })?
            .ok_or_else(|| {
                CoreError::Internal("a lesson with no fingerprint was not written".into())
            })
    }

    /// Source 1: the same loop-guard fingerprint on two tasks of one repository within 60 days. Called after a
    /// failed verification is recorded; it never fails the task.
    pub(super) fn learn_repeated_failure(
        &self,
        task: &CrewTask,
        fingerprint: &str,
        judged: &Judgement,
    ) {
        if !is_guard_fingerprint(fingerprint) {
            return;
        }
        let Some(text) = fingerprint_text(&judged.failures) else {
            return;
        };
        let Ok(projects) = self
            .store
            .crew_projects_of_tasks(std::slice::from_ref(&task.id))
        else {
            return;
        };
        let Some(project) = projects.get(&task.id).cloned() else {
            return;
        };
        let now = unix_now();
        if !within_window(&task.created_at, now) {
            return;
        }
        let Ok(peers) = self.store.crew_tasks_with_fingerprint(
            &project,
            fingerprint,
            now - WINDOW_SECS,
            &task.id,
        ) else {
            return;
        };
        if peers.len() + 1 < THRESHOLD {
            return;
        }
        // Five ids in all: the peers, then the task this lesson was learned on.
        let mut evidence: Vec<LessonEvidence> = peers
            .iter()
            .take(EVIDENCE_MAX.saturating_sub(1))
            .map(|id| LessonEvidence {
                kind: "task".to_owned(),
                id: id.clone(),
            })
            .collect();
        evidence.push(LessonEvidence {
            kind: "task".to_owned(),
            id: task.id.clone(),
        });
        let draft = LessonDraft {
            scope: LessonScope::Repo(project.clone()),
            source: "fingerprint",
            text,
            fingerprint: Some(fingerprint_key(&project, fingerprint)),
            evidence,
        };
        if let Err(error) = self.write_lesson(draft) {
            tracing::warn!(%error, "could not store a repeated-failure lesson");
        }
    }

    /// Source 2: a person confirmed an escape (the escape log's own reason, or its source when it was a revert).
    /// Called after the confirm; it never fails the person's act.
    pub(super) fn learn_confirmed_escape(&self, escape: &EscapeRecord) {
        if escape.state != EscapeState::Confirmed {
            return;
        }
        let reverted = escape.source == EscapeSource::Revert;
        let text = escape_text(
            escape.reason.as_deref().unwrap_or_default(),
            &escape.areas,
            reverted,
        );
        let fingerprint = if reverted {
            merge_key(&escape.merge_task_id)
        } else {
            escape_key(&escape.id)
        };
        let draft = LessonDraft {
            scope: LessonScope::Repo(escape.project_id.clone()),
            source: if reverted { "revert" } else { "escape" },
            text,
            fingerprint: Some(fingerprint),
            evidence: escape_evidence(&escape.id, &escape.merge_task_id),
        };
        if let Err(error) = self.write_lesson(draft) {
            tracing::warn!(%error, "could not store a confirmed-escape lesson");
        }
    }

    /// Source 2, the person's own revert: a merge a person reverted is a lesson at once, from the escape it logged.
    /// A revert the crew made writes nothing here; only a person's confirm of that escape writes it.
    pub(super) fn learn_person_merge_revert(&self, merge_task: &str) {
        let Ok(Some(escape)) = self.store.crew_escape_of_merge(merge_task) else {
            return;
        };
        if escape.source != EscapeSource::Revert {
            return;
        }
        let draft = LessonDraft {
            scope: LessonScope::Repo(escape.project_id.clone()),
            source: "revert",
            text: escape_text(
                escape.reason.as_deref().unwrap_or_default(),
                &escape.areas,
                true,
            ),
            fingerprint: Some(merge_key(merge_task)),
            evidence: escape_evidence(&escape.id, &escape.merge_task_id),
        };
        if let Err(error) = self.write_lesson(draft) {
            tracing::warn!(%error, "could not store a person's-revert lesson");
        }
    }

    /// The one write path: cleaned text, the secret scan, the path rule, then the store's transaction. Every
    /// learned source comes through here; none can write a rule.
    fn write_lesson(&self, draft: LessonDraft) -> Result<Option<CrewLesson>> {
        let text = clean_generated(&draft.text).ok_or_else(|| {
            CoreError::Invalid(format!("A lesson is 1 to {TEXT_MAX} characters."))
        })?;
        refuse_secret_text(&text)?;
        let names_path = names_path(&text);
        if names_path && draft.scope.project().is_none() {
            return Err(CoreError::Invalid(
                "A global lesson names path classes, not paths.".into(),
            ));
        }
        self.store.write_crew_lesson(NewCrewLesson {
            scope: draft.scope.word().to_owned(),
            project_id: draft.scope.project().map(str::to_owned),
            source: draft.source.to_owned(),
            text,
            fingerprint: draft.fingerprint,
            evidence: draft.evidence,
            edited_by_person: draft.source == "person",
            names_path,
        })
    }
}

/// Source 1's sentence: the failing checks by kind, where they failed and the guard's safe failure sentence.
/// A check's id, its command or any other part of the key never enters the text (H1); only its kind does.
fn fingerprint_text(failures: &[Failed]) -> Option<String> {
    let mut kinds: Vec<String> = failures
        .iter()
        .map(|failure| one_line(&failure.kind.word()))
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    if kinds.is_empty() {
        return None;
    }
    let kinds: Vec<String> = kinds
        .iter()
        .take(3)
        .map(|kind| format!("`{kind}`"))
        .collect();
    let area = failures
        .iter()
        .flat_map(|failure| failure.names.iter())
        .filter_map(|name| path_area(name))
        .min();
    let where_it_failed = match (
        area,
        failures.iter().any(|failure| !failure.names.is_empty()),
    ) {
        (Some(area), _) => format!("files under `{area}`"),
        (None, true) => "the tests".to_owned(),
        (None, false) => "the checks".to_owned(),
    };
    let sentence = one_line(&failures[0].reason);
    clean_generated(&format!(
        "Checks like {} failed on {where_it_failed} before: {sentence}",
        kinds.join(", ")
    ))
}

/// Source 2's sentence: where it was, whether it escaped or was reverted, and the recorded reason.
fn escape_text(reason: &str, areas: &[String], reverted: bool) -> String {
    let area = areas
        .iter()
        .map(|area| one_line(area))
        .find(|area| !area.is_empty())
        .map(|area| format!(" in `{area}`"))
        .unwrap_or_default();
    let verb = if reverted { "was reverted" } else { "escaped" };
    let reason = one_line(reason);
    let reason = if reverted && reason == REVERT_DEFAULT_REASON {
        String::new()
    } else {
        reason
    };
    let text = if reason.is_empty() {
        format!("A change{area} {verb}.")
    } else {
        format!("A change{area} {verb}: {reason}")
    };
    clean_generated(&text).unwrap_or_else(|| format!("A change{area} {verb}."))
}

/// The dedupe key of a repeated failure: one repository's one fingerprint.
fn fingerprint_key(project: &str, fingerprint: &str) -> String {
    sha256_hex(format!("fingerprint:{project}:{fingerprint}").as_bytes())
}

/// The dedupe key of a confirmed escape that is not a revert.
fn escape_key(id: &str) -> String {
    sha256_hex(format!("escape:{id}").as_bytes())
}

/// The dedupe key of a reverted merge: the person's revert and a later confirm of its escape are one lesson.
fn merge_key(merge_task: &str) -> String {
    sha256_hex(format!("merge:{merge_task}").as_bytes())
}

fn escape_evidence(escape_id: &str, merge_task: &str) -> Vec<LessonEvidence> {
    vec![
        LessonEvidence {
            kind: "escape".to_owned(),
            id: escape_id.to_owned(),
        },
        LessonEvidence {
            kind: "merge".to_owned(),
            id: merge_task.to_owned(),
        },
    ]
}

/// The loop guard's fingerprint, and nothing like the no-report sentinel.
fn is_guard_fingerprint(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Whether a task created then is inside the 60-day window ending now.
fn within_window(created_at: &str, now: i64) -> bool {
    parse_time(created_at).is_some_and(|at| at >= now - WINDOW_SECS)
}

/// A person's own words: trimmed, 1 to 300 characters, no character that hides text, secret-scanned before any
/// redaction so a secret is refused rather than quietly masked.
fn person_text(words: &str) -> Result<String> {
    refuse_secret_text(words)?;
    super::background::text("lesson", words, TEXT_MAX)
        .map_err(|error| CoreError::Invalid(error.public_message()))
}

/// The secret scan of a lesson's text: a secret is refused, never stored.
fn refuse_secret_text(text: &str) -> Result<()> {
    if super::guard::find_secret(&serde_json::json!({ "text": text })).is_some() {
        return Err(CoreError::Refused {
            status: 400,
            code: "secret_refused",
            message: SECRET_REFUSAL.to_owned(),
            detail: serde_json::Value::Null,
        });
    }
    Ok(())
}

/// Whether a lesson's text names a repository-relative path: a `/` path that `path_problem` accepts, or a file name
/// with an alphabetic extension of at least two letters. A URL is not a path.
fn names_path(text: &str) -> bool {
    text.split_whitespace()
        .map(token)
        .filter(|token| !token.is_empty())
        .any(|token| {
            if token.contains("://") {
                return false;
            }
            if token.contains('/') {
                // Path syntax of any kind names a path: a rejected one (absolute, `..`) is still not path-free text,
                // and a global lesson may not carry it.
                return true;
            }
            file_name(token)
        })
}

fn token(text: &str) -> &str {
    text.trim_matches(|c: char| !(c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-')))
        // A file name at the end of a sentence keeps no full stop.
        .trim_end_matches('.')
}

fn file_name(token: &str) -> bool {
    let Some((stem, extension)) = token.rsplit_once('.') else {
        return false;
    };
    let letters = extension.chars().count();
    // One letter is an extension only after a stem of two or more (`parser.c`), which keeps `e.g.` and `i.e.` out.
    !stem.is_empty()
        && (2..=10).contains(&letters) | (letters == 1 && stem.chars().count() >= 2)
        && extension.chars().all(|c| c.is_ascii_alphabetic())
}

/// The folder class of a name that is a repository-relative path, if it is one: the area
/// `area_of` names, and the folder itself when that area is the file.
fn path_area(name: &str) -> Option<String> {
    let token = token(name.trim());
    (token.contains('/')
        && !token.contains(char::is_whitespace)
        && super::classes::path_problem(token).is_none())
    .then(|| {
        let area = super::autopilot::area_of(token);
        if area == token {
            token
                .rsplit_once('/')
                .map_or(area.clone(), |(folder, _)| folder.to_owned())
        } else {
            area
        }
    })
}

/// Text a person can read: one line of at most 300 characters, no control or hidden characters.
fn clean_generated(text: &str) -> Option<String> {
    let one_line = one_line(text);
    let cleaned: String = one_line
        .chars()
        .filter(|c| !c.is_control() && !super::background::invisible(*c))
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    Some(cap_chars(cleaned, TEXT_MAX))
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cap_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(3)).collect();
    cut.push_str("...");
    cut
}
