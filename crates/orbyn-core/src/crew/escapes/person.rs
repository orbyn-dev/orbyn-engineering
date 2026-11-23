//! A person's acts on the escape log (slice 4k, PR 2): mark a merge, confirm, dismiss, set the gap. Each is the person's choice
//! alone: the routes are `Person` and enforced (the desktop shell's key), in `crew::changes::STATE_CHANGING` so an agent's request is
//! refused by path (N1), and no tool, CLI command or scheduler reaches them. **Only a person's confirm (or mark) makes an escape
//! `confirmed`**; the core's own detection writes `possible` only, and nothing here feeds the breaker, the Router or any gate (a
//! confirmed escape is display only in v1, docs/crew/escapes.md). The rules of each transition are in `store::crew_escape_person`.

use orbyn_contracts as wire;

use super::view::row_wire;
use crate::app::Core;
use crate::crew::autopilot::valid_id;
use crate::crew::redact;
use crate::error::{CoreError, Result};
use crate::store::EscapeDecision;
use crate::util::unix_now;

/// The longest note, in characters.
const NOTE_MAX: usize = 200;

fn note_of(note: Option<&str>) -> Result<Option<String>> {
    let Some(text) = note.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > NOTE_MAX {
        return Err(CoreError::Invalid(format!(
            "A note is at most {NOTE_MAX} characters."
        )));
    }
    Ok(Some(redact(text).chars().take(NOTE_MAX).collect()))
}

impl Core {
    fn escape_scope(&self, project: &str, id: &str) -> Result<()> {
        self.crew_preview_only()?;
        valid_id(project)?;
        valid_id(id)?;
        self.store.project(project)?;
        Ok(())
    }

    /// `POST /api/crew/escapes/{id}/confirm`: a person confirms a possible escape. Confirmed again changes nothing.
    pub fn crew_escape_confirm(
        &self,
        id: &str,
        body: &wire::EscapeDecide,
    ) -> Result<wire::EscapeRow> {
        self.crew_escape_decide(id, body, EscapeDecision::Confirm)
    }

    /// `POST /api/crew/escapes/{id}/dismiss`: a person dismisses a possible escape. It is not offered again.
    pub fn crew_escape_dismiss(
        &self,
        id: &str,
        body: &wire::EscapeDecide,
    ) -> Result<wire::EscapeRow> {
        self.crew_escape_decide(id, body, EscapeDecision::Dismiss)
    }

    fn crew_escape_decide(
        &self,
        id: &str,
        body: &wire::EscapeDecide,
        decision: EscapeDecision,
    ) -> Result<wire::EscapeRow> {
        self.escape_scope(&body.project, id)?;
        let note = note_of(body.note.as_deref())?;
        let record = self.store.decide_escape(
            &body.project,
            id,
            decision,
            &body.seen,
            note.as_deref(),
            unix_now(),
        )?;
        // A person's confirm is the escape source: the lesson is written after the log has it, and never blocks the confirm.
        if decision == EscapeDecision::Confirm {
            self.learn_confirmed_escape(&record);
        }
        Ok(row_wire(&record))
    }

    /// `POST /api/crew/escapes/mark`: a person marks a merge as an escape; the mark is the confirm.
    pub fn crew_escape_mark(&self, body: &wire::EscapeMark) -> Result<wire::EscapeRow> {
        self.escape_scope(&body.project, &body.merge_task_id)?;
        let note = note_of(body.note.as_deref())?;
        let record = self.store.mark_escape(
            &body.project,
            &body.merge_task_id,
            body.seen.as_ref(),
            note.as_deref(),
            unix_now(),
        )?;
        // A mark is a confirm: the escape source writes its lesson, once per escape.
        self.learn_confirmed_escape(&record);
        Ok(row_wire(&record))
    }

    /// `POST /api/crew/escapes/{id}/gap`: a person sets the gap of a possible or confirmed escape.
    pub fn crew_escape_set_gap(
        &self,
        id: &str,
        body: &wire::EscapeGapSet,
    ) -> Result<wire::EscapeRow> {
        self.escape_scope(&body.project, id)?;
        let record =
            self.store
                .set_escape_gap(&body.project, id, &body.seen, body.gap, unix_now())?;
        Ok(row_wire(&record))
    }
}
