//! The exit status of a guarded command, best effort. When a command the guard held goes through (the person's one-time approval
//! was used, or in Full both keys said yes), the ledger watches the worker's thread for the engine's own record of that command
//! finishing, for ten minutes. When it arrives the ledger appends a `command_exit` row with the exit code; when it does not, the row
//! says `unknown`, which the app draws as "Exit status unknown". It is a watch, not a promise: some engines report no exit code at
//! all, and a command the engine wrapped in a shell is matched by its text, so a mismatch is `unknown` too.
//!
//! The watches live in memory (the raw command may hold a secret and is never written to disk); the rows they write hold the
//! redacted command.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde_json::{Value, json};

use super::ledger;
use super::shell::simple_commands;
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
    /// Whether the one reaper task is running. It sleeps between sweeps and ends when no watch is left, so open watches never cost
    /// a task each.
    reaper: AtomicBool,
}

impl ExitWatches {
    fn lock(&self) -> MutexGuard<'_, Vec<Watch>> {
        self.items.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ends the reaper if no watch is left. Done under the lock the pushes take, so a watch added at the same moment starts a new one.
    fn reaper_done_if_empty(&self) -> bool {
        let items = self.lock();
        if items.is_empty() {
            self.reaper.store(false, Ordering::SeqCst);
            true
        } else {
            false
        }
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

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The command an engine ran, with one shell wrapper taken off (`zsh -lc '...'`) and whitespace collapsed.
fn engine_command(text: &str) -> String {
    const SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "ksh"];
    // The wrapped command is read before the shell that wraps it, so the shell is found by its program, not by its place.
    if let Some(first) = simple_commands(text)
        .iter()
        .find(|c| SHELLS.contains(&c.program.as_str()))
        && let Some(at) = first
            .args
            .iter()
            .position(|a| matches!(a.as_str(), "-c" | "-lc" | "-ic" | "-cl"))
        && let Some(inner) = first.args.get(at + 1)
    {
        return collapse(inner);
    }
    collapse(text)
}

/// Whether the engine's command is the one watched: the same text once whitespace is collapsed and one shell wrapper is off. Not a
/// substring: another command that merely holds this one is not it.
fn same_command(engine: &str, watched: &str) -> bool {
    let watched = collapse(watched);
    !watched.is_empty() && engine_command(engine) == watched
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
        // The ten minutes end by themselves: a command whose exit never arrives still gets its `unknown` row. One task reaps them all.
        if !self
            .crew_autopilot
            .exits
            .reaper
            .swap(true, Ordering::SeqCst)
        {
            let weak = std::sync::Arc::downgrade(self);
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        let Some(core) = weak.upgrade() else { break };
                        core.exit_sweep(unix_now());
                        if core.crew_autopilot.exits.reaper_done_if_empty() {
                            break;
                        }
                    }
                });
            } else {
                self.crew_autopilot
                    .exits
                    .reaper
                    .store(false, Ordering::SeqCst);
            }
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
