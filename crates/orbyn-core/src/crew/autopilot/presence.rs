//! The service's half of approving a held command in the desktop app. **Nothing here approves.** It answers what the shell asks
//! on the channel only the shell holds and records the person's grant; the approval itself is `held`'s, and needs the grant.
//!
//! The messages are newline JSON, as the rest of that channel (`host::shell`):
//!
//! | The shell sends | The service answers on stdout |
//! | --- | --- |
//! | `presence_describe {request_id, hold_id}` | `presence_hold {request_id, hold_id, thread_id, task_id, command_shown, command_sha256, worker_name, task_title}` or `presence_refused {request_id, code}` |
//! | `presence_grant {request_id, nonce, hold_id, thread_id, task_id, command_sha256}` | `presence_ready {request_id}` or `presence_refused {request_id, code}` |
//!
//! The command, the worker's name and the task's title all come from here, redacted and bounded, never from the window: the
//! dialog shows what the service says the hold is. `command_sha256` is the hold's binding hash (the task, the thread, the kind,
//! the whole command and its details). A grant is recorded only if the hold is open now and every field matches it as it stands
//! now; the shell hands the nonce to the page only after `presence_ready`.
//!
//! Refusal codes: `unavailable` (the Preview flag is off or no shell is attached), `invalid` (a field is malformed), `not_found`,
//! `gone` (decided, expired or its task ended), `unreadable` (too long to read whole), `mismatch` (a bound field changed).

use serde_json::{Value, json};

use crate::host::presence::{PresenceFor, is_nonce};
use crate::host::shell::is_id;
use crate::util::unix_now;

use super::held::{Failure, Stage};
use crate::app::Core;

/// The longest worker name and task title the dialog shows, in characters.
pub const WORKER_NAME_MAX: usize = 80;
pub const TASK_TITLE_MAX: usize = 120;

/// What the dialog shows about a hold, as the service has it now.
struct Facts {
    hold: String,
    thread: String,
    task: String,
    shown: String,
    bound: String,
    worker: String,
    title: String,
}

/// Text for the dialog: redacted, the answer key's text withheld, cut to `max` characters, and empty if a secret or the key's
/// text would still show.
fn bounded(text: &str, max: usize, needles: &[String]) -> String {
    let clean = crate::crew::withhold(&crate::crew::redact(text), needles);
    let cut: String = clean.chars().take(max).collect();
    let value = json!(cut);
    if crate::crew::refuse_secret(&value).is_ok()
        && crate::crew::refuse_leak(&value, needles).is_ok()
    {
        cut
    } else {
        String::new()
    }
}

impl Core {
    /// Takes a line meant for this side. False leaves it for the next reader.
    pub fn presence_line(&self, line: &str) -> bool {
        self.presence_line_at(line, unix_now())
    }

    pub(crate) fn presence_line_at(&self, line: &str, now: i64) -> bool {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        let text = |key: &str| message.get(key).and_then(Value::as_str).unwrap_or_default();
        let kind = text("type");
        if kind != "presence_describe" && kind != "presence_grant" {
            return false;
        }
        let request = text("request_id");
        if !is_id(request) {
            tracing::warn!("ignoring a presence message without a usable request id");
            return true;
        }
        let answer = if kind == "presence_describe" {
            self.presence_describe(request, text("hold_id"), now)
        } else {
            self.presence_grant(request, &message, now)
        };
        let answer = answer.unwrap_or_else(
            |code| json!({ "type": "presence_refused", "request_id": request, "code": code }),
        );
        if !self.shell.send(answer) {
            tracing::warn!("a presence answer found no shell to go to");
        }
        true
    }

    fn presence_describe(
        &self,
        request: &str,
        hold: &str,
        now: i64,
    ) -> Result<Value, &'static str> {
        let facts = self.presence_facts(hold, now)?;
        Ok(json!({
            "type": "presence_hold",
            "request_id": request,
            "hold_id": facts.hold,
            "thread_id": facts.thread,
            "task_id": facts.task,
            "command_shown": facts.shown,
            "command_sha256": facts.bound,
            "worker_name": facts.worker,
            "task_title": facts.title,
        }))
    }

    fn presence_grant(
        &self,
        request: &str,
        message: &Value,
        now: i64,
    ) -> Result<Value, &'static str> {
        let text = |key: &str| message.get(key).and_then(Value::as_str).unwrap_or_default();
        if !self.shell.attached() {
            return Err("unavailable");
        }
        let nonce = text("nonce");
        if !is_nonce(nonce) {
            return Err("invalid");
        }
        let facts = self.presence_facts(text("hold_id"), now)?;
        if text("thread_id") != facts.thread
            || text("task_id") != facts.task
            || text("command_sha256") != facts.bound
        {
            return Err("mismatch");
        }
        self.presence.put(
            PresenceFor::CrewApproveOnce {
                hold: facts.hold,
                thread: facts.thread,
                task: facts.task,
                command_sha256: facts.bound,
            },
            nonce.to_owned(),
            now,
        );
        Ok(json!({ "type": "presence_ready", "request_id": request }))
    }

    /// The hold as it stands now, if a person may still approve it.
    fn presence_facts(&self, hold: &str, now: i64) -> Result<Facts, &'static str> {
        if self.crew_preview_only().is_err() || !self.shell.attached() {
            return Err("unavailable");
        }
        if super::merge::valid_id(hold).is_err() {
            return Err("invalid");
        }
        self.held_sweep(now);
        let held = self
            .held_book()
            .open(hold)
            .map_err(|failure| match failure {
                Failure::Unknown => "not_found",
                Failure::Over | Failure::Decided(_) => "gone",
            })?;
        debug_assert_eq!(held.stage, Stage::Held);
        let Some((_, run)) = self
            .live_worker(&held.thread_id)
            .filter(|(task, _)| task.id == held.task_id)
        else {
            self.held_ended_by_task(hold);
            return Err("gone");
        };
        if !held.readable {
            return Err("unreadable");
        }
        let needles = self.held_needles(&held.task_id);
        let title = self
            .store
            .crew_brief(&held.task_id)
            .ok()
            .flatten()
            .map(|brief| brief.outcome)
            .unwrap_or_default();
        Ok(Facts {
            hold: held.id.clone(),
            thread: held.thread_id.clone(),
            task: held.task_id.clone(),
            shown: held.shown.clone(),
            bound: held.bound.clone(),
            worker: bounded(&run.member, WORKER_NAME_MAX, &needles),
            title: bounded(&title, TASK_TITLE_MAX, &needles),
        })
    }
}
