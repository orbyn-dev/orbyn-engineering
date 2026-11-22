//! The last reason an engine did not start, by engine id, kept for the status. Like the
//! ACP sign-in capture this outlives a run and a status refresh: only a clean start clears
//! it. Messages are written by the core, never by the agent, and are truncated here as a
//! last guard. The lock is never held across an await and a poisoned one is recovered.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};

use orbyn_contracts::{EngineStartIssue, EngineStartIssueKind};

/// The longest message kept, in characters. The core writes short sentences; this only
/// bounds a mistake.
const MAX_MESSAGE_CHARS: usize = 300;

static ISSUES: LazyLock<Mutex<HashMap<String, EngineStartIssue>>> = LazyLock::new(Default::default);

fn issues() -> MutexGuard<'static, HashMap<String, EngineStartIssue>> {
    ISSUES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Records the last start failure for `engine_id`, replacing any earlier one.
pub fn note_start_issue(engine_id: &str, kind: EngineStartIssueKind, message: &str) {
    let issue = EngineStartIssue {
        kind,
        message: message.chars().take(MAX_MESSAGE_CHARS).collect(),
        at: crate::util::now(),
    };
    issues().insert(engine_id.to_owned(), issue);
}

/// Forgets what was recorded for `engine_id`: the next status has no start issue.
pub fn clear_start_issue(engine_id: &str) {
    issues().remove(engine_id);
}

/// The last start failure for `engine_id`, when nothing has cleared it.
pub fn start_issue(engine_id: &str) -> Option<EngineStartIssue> {
    issues().get(engine_id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn note_then_read_reports_the_kind_message_and_time() {
        let id = "start-issue-note";
        note_start_issue(id, EngineStartIssueKind::SignedOut, "Sign in again.");
        let issue = start_issue(id).expect("the note is readable");
        assert_eq!(issue.kind, EngineStartIssueKind::SignedOut);
        assert_eq!(issue.message, "Sign in again.");
        clear_start_issue(id);
    }

    #[test]
    fn clear_removes_the_note() {
        let id = "start-issue-clear";
        note_start_issue(id, EngineStartIssueKind::Failed, "It did not start.");
        clear_start_issue(id);
        assert_eq!(start_issue(id), None);
    }

    #[test]
    fn a_second_note_replaces_the_first() {
        let id = "start-issue-replace";
        note_start_issue(id, EngineStartIssueKind::SignedOut, "First.");
        note_start_issue(id, EngineStartIssueKind::Failed, "Second.");
        let issue = start_issue(id).unwrap();
        assert_eq!(issue.kind, EngineStartIssueKind::Failed);
        assert_eq!(issue.message, "Second.");
        clear_start_issue(id);
    }

    #[test]
    fn a_long_message_is_truncated_to_300_characters() {
        let id = "start-issue-long";
        note_start_issue(id, EngineStartIssueKind::Failed, &"x".repeat(400));
        let message = start_issue(id).unwrap().message;
        assert_eq!(message.chars().count(), 300);
        assert_eq!(message, "x".repeat(300));
        clear_start_issue(id);
    }

    #[test]
    fn at_is_an_rfc3339_utc_time() {
        let id = "start-issue-at";
        note_start_issue(id, EngineStartIssueKind::Failed, "It did not start.");
        let at = start_issue(id).unwrap().at;
        let parsed =
            time::OffsetDateTime::parse(&at, &time::format_description::well_known::Rfc3339)
                .expect("the timestamp parses as RFC 3339");
        assert_eq!(parsed.offset(), time::UtcOffset::UTC);
        clear_start_issue(id);
    }

    #[test]
    fn two_threads_note_and_read_without_deadlocking() {
        let id = "start-issue-concurrent";
        let (done, finished) = std::sync::mpsc::channel();
        let writer_done = done.clone();
        let writer = std::thread::spawn(move || {
            for count in 0..200 {
                note_start_issue(
                    id,
                    EngineStartIssueKind::Failed,
                    &format!("Failure {count}."),
                );
                let _ = start_issue(id);
            }
            clear_start_issue(id);
            writer_done.send(()).unwrap();
        });
        let reader = std::thread::spawn(move || {
            for _ in 0..200 {
                let _ = start_issue(id);
            }
            done.send(()).unwrap();
        });
        for _ in 0..2 {
            finished
                .recv_timeout(Duration::from_secs(5))
                .expect("the start issue map deadlocked");
        }
        writer.join().unwrap();
        reader.join().unwrap();
        assert_eq!(start_issue(id), None);
    }
}
