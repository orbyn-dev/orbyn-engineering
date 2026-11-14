//! The exit status of a guarded command, best effort. When a command the guard held goes through (the person's one-time approval
//! was used, or in Full both keys said yes), the ledger watches the worker's thread for the engine's own record of that command
//! finishing, for ten minutes. When it arrives the ledger appends a `command_exit` row with the exit code; when it does not, the row
//! says `unknown`, which the app draws as "Exit status unknown". It is a watch, not a promise: some engines report no exit code at
//! all, and a command the engine wrapped in a shell is matched by its text, so a mismatch is `unknown` too.
//!
//! The watches live in memory (the raw command may hold a secret and is never written to disk); the rows they write hold the
//! redacted command.

use std::sync::{Mutex, MutexGuard, PoisonError};

use serde_json::{Value, json};

use super::ledger;
use crate::agents::driver::ItemRecord;
use crate::app::Core;
use crate::governance::approvals::ActionAsk;
use crate::store::CrewTask;
use crate::util::{sha256_hex, unix_now};

/// How long the watch waits for the engine to report the command finished, in seconds.
pub const EXIT_WATCH_SECS: i64 = 10 * 60;
/// Watches kept at once; the oldest go first (their rows say `unknown`).
const KEPT: usize = 200;

struct Watch {
    task_id: String,
    crew_id: String,
    thread_id: String,
    /// The exact command, trimmed. Memory only.
    command: String,
    hold: Option<String>,
    /// The ledger row that let the command through.
    row: Option<i64>,
    guard: Option<Value>,
    deadline: i64,
}

#[derive(Default)]
pub struct ExitWatches {
    items: Mutex<Vec<Watch>>,
}

impl ExitWatches {
    fn lock(&self) -> MutexGuard<'_, Vec<Watch>> {
        self.items.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Watches open now (for tests).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.lock().len()
    }
}

/// The command text an engine's record of an item holds, if it is a command: `command` or `input.command` as a string, or a
/// word list.
fn command_of(raw: &Value) -> Option<String> {
    for pointer in ["/command", "/input/command", "/input/cmd", "/cmd"] {
        match raw.pointer(pointer) {
            Some(Value::String(text)) => return Some(text.trim().to_owned()),
            Some(Value::Array(words)) => {
                let words: Vec<&str> = words.iter().filter_map(Value::as_str).collect();
                if !words.is_empty() {
                    return Some(words.join(" "));
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the engine's command is the one watched. An engine may run it through a shell (`zsh -lc '...'`), so the watched text
/// being inside the engine's text is a match; an empty text matches nothing.
fn same_command(engine: &str, watched: &str) -> bool {
    !engine.is_empty() && !watched.is_empty() && (engine == watched || engine.contains(watched))
}

impl Core {
    /// Starts watching for the exit status of a guarded command that was let through. The command is `ask.target`; `hold` is the
    /// approval that let it through, if it was one, and `row` the ledger row that says so.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn exit_watch(
        self: &std::sync::Arc<Self>,
        task: &CrewTask,
        thread_id: &str,
        ask: &ActionAsk,
        hold: Option<&str>,
        row: Option<i64>,
        guard: Option<&Value>,
        now: i64,
    ) {
        self.exit_sweep(now);
        let command = ask.target.trim().to_owned();
        let overflow = {
            let mut items = self.crew_autopilot.exits.lock();
            items.push(Watch {
                task_id: task.id.clone(),
                crew_id: task.crew_id.clone(),
                thread_id: thread_id.to_owned(),
                command,
                hold: hold.map(str::to_owned),
                row,
                guard: guard.cloned(),
                deadline: now + EXIT_WATCH_SECS,
            });
            let mut gone = Vec::new();
            while items.len() > KEPT {
                gone.push(items.remove(0));
            }
            gone
        };
        for watch in overflow {
            self.exit_row(&watch, None);
        }
        // The ten minutes end by themselves: a command whose exit never arrives still gets its `unknown` row.
        let weak = std::sync::Arc::downgrade(self);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(
                    u64::try_from(EXIT_WATCH_SECS).unwrap_or(600) + 1,
                ))
                .await;
                if let Some(core) = weak.upgrade() {
                    core.exit_sweep(unix_now());
                }
            });
        }
    }

    /// The engine's record of an item on a worker's thread. If it is a finished command that a watch is waiting for, the exit
    /// status goes to the ledger.
    pub(crate) fn guard_exit_seen(&self, thread_id: &str, item: &ItemRecord) {
        self.exit_sweep(unix_now());
        let Some(code) = item.exit_code else { return };
        let Some(engine) = command_of(&item.raw) else {
            return;
        };
        let found = {
            let mut items = self.crew_autopilot.exits.lock();
            items
                .iter()
                .position(|w| w.thread_id == thread_id && same_command(&engine, &w.command))
                .map(|at| items.remove(at))
        };
        if let Some(watch) = found {
            self.exit_row(&watch, Some(code));
        }
    }

    /// Ends every watch whose ten minutes ran out, with the status `unknown`.
    pub(crate) fn exit_sweep(&self, now: i64) {
        let ended: Vec<Watch> = {
            let mut items = self.crew_autopilot.exits.lock();
            let mut ended = Vec::new();
            let mut at = 0;
            while at < items.len() {
                if items[at].deadline <= now {
                    ended.push(items.remove(at));
                } else {
                    at += 1;
                }
            }
            ended
        };
        for watch in ended {
            self.exit_row(&watch, None);
        }
    }

    /// One `command_exit` row: the hold (or `null`), the ledger row that let the command through, the redacted command, and
    /// `exit_code` as a number or exactly `unknown`.
    fn exit_row(&self, watch: &Watch, code: Option<i64>) {
        let needles = self.held_needles(&watch.task_id);
        let mut body = json!({
            "hold": watch.hold,
            "row": watch.row,
            "decided_by": "autopilot",
            "exit_code": code.map_or_else(|| json!("unknown"), |c| json!(c)),
            "bound": format!("sha256:{}", sha256_hex(watch.command.as_bytes())),
            "action": { "kind": "command", "target": ledger::shown(&watch.command, &needles), "paths": [] },
        });
        if let Some(guard) = &watch.guard {
            body["guard"] = guard.clone();
        }
        if let Err(error) = self.store.add_crew_autopilot_event(
            &watch.task_id,
            &watch.crew_id,
            "command_exit",
            &ledger::safe(&body, &needles),
        ) {
            tracing::warn!(%error, "autopilot could not write the exit status of a guarded command");
        }
    }
}
