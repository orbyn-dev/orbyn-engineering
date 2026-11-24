//! Watches (docs/crew.md, "Kiro Crew features" KC6, slices 2 and 3): a background crew's fixed command run on its own schedule
//! with no model call. The command runs in the project's checkout for at most five minutes, screened again before every check
//! (the always-denied list and the blast-radius guard, whose lists may have changed since the crew was made), and its last 20
//! lines and 2 KiB are kept, redacted; nothing else of its output is stored. **The first check that runs is a baseline and raises
//! nothing.** After it, only a turn between passing and failing is a change: a turn to failing raises one Needs you row or starts
//! the crew's template once, and a turn to passing writes "Passing again" and resolves the row. A different failure while it
//! stays failing, and a repeat of the same result, update the last exit code and tail and write no row. A check that could not
//! run counts a streak: three in a row raise one Needs you row, and a check that runs again resolves it. A command that could not
//! run never raises by itself.
//!
//! **No model call while nothing changes.** The runner runs the command only; a start happens only through
//! `crew_background_start`, the same seam Run now and the other listeners take, and only when a result turns failing with the
//! trigger's `on_failing` set to `template`. A start that is refused (the crew already running, the daily budget, an untrusted
//! folder, no engine) leaves the Needs you row raised, writes the same `crew.background.listener.refused` audit row the other
//! listeners write, and records why on the change row as one of the seven fixed codes below; the wire turns the code into the
//! fixed sentence a person reads.
//!
//! The state machine is the pure `plan` function below, applied by the watch tick in `background_listeners`; the reads are
//! views of the two tables migration 77 made, and migration 87 gave a change row its refusal code.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use orbyn_contracts as wire;
use orbyn_contracts::CheckOutcome;

use super::autopilot::Screened;
use super::background::{next_offset, parse_wire};
use super::checks::{self, COULD_NOT_START, RAN_OUT_OF_TIME, TOO_MUCH_OUTPUT};
use super::{Page, redact, valid_id};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{BackgroundRow, NewWatchChange, WatchChangeRow, WatchStateRow};
use crate::util::format_unix;

/// No check of a watch runs longer than this.
pub const CHECK_LIMIT: Duration = Duration::from_secs(5 * 60);
/// How many lines of a check's output are kept.
pub const TAIL_LINES: usize = 20;
/// How many bytes of that output are kept.
pub const TAIL_BYTES: usize = 2 * 1024;
/// The most a check may print on each of its two streams: a command over it is stopped and could not run. A check is a status
/// command, not a log stream, and this keeps a periodic check from filling memory.
const OUTPUT_CAP: usize = 1024 * 1024;
/// The most of a check's raw log is read before it is redacted and cut, so a command that prints megabytes costs at most this.
const RAW_KEEP: usize = 64 * 1024;

/// Why a check could not run, in the fixed words the state row and the person read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CouldNotRun {
    TimedOut,
    CouldNotStart,
    /// The command the screen refuses now, though it was accepted when the watch was made.
    Refused,
    /// The project's folder is no longer trusted, so nothing runs in it.
    Untrusted,
    /// The command printed more than a check may buffer, so it was stopped.
    TooMuchOutput,
}

impl CouldNotRun {
    /// The word stored in `crew_watch_state.last_reason`.
    pub fn word(self) -> &'static str {
        match self {
            Self::TimedOut => "timed_out",
            Self::CouldNotStart => "could_not_start",
            Self::Refused => "refused",
            Self::Untrusted => "untrusted",
            Self::TooMuchOutput => "too_much_output",
        }
    }

    /// The same reason as a person reads it.
    pub fn sentence(self) -> &'static str {
        match self {
            Self::TimedOut => "timed out",
            Self::CouldNotStart => "could not start",
            Self::Refused => "refused",
            Self::Untrusted => "folder not trusted",
            Self::TooMuchOutput => "printed too much",
        }
    }

    /// The reason a stored word names, for a row this code wrote.
    pub fn of_word(word: &str) -> Option<Self> {
        match word {
            "timed_out" => Some(Self::TimedOut),
            "could_not_start" => Some(Self::CouldNotStart),
            "refused" => Some(Self::Refused),
            "untrusted" => Some(Self::Untrusted),
            "too_much_output" => Some(Self::TooMuchOutput),
            _ => None,
        }
    }
}

/// What one check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchFound {
    /// The command ran: it passed when it exited 0.
    Ran {
        pass: bool,
        exit_code: Option<u32>,
    },
    CouldNotRun(CouldNotRun),
}

/// One check of one watch: what it found and the redacted tail it kept.
#[derive(Clone, Debug, PartialEq)]
pub struct WatchCheck {
    pub found: WatchFound,
    /// The output, already redacted and cut; None when nothing ran or it printed nothing.
    pub tail: Option<String>,
}

impl WatchCheck {
    pub fn could_not_run(why: CouldNotRun) -> Self {
        Self {
            found: WatchFound::CouldNotRun(why),
            tail: None,
        }
    }

    fn exit_code(&self) -> Option<i64> {
        match self.found {
            WatchFound::Ran { exit_code, .. } => exit_code.map(i64::from),
            WatchFound::CouldNotRun(_) => None,
        }
    }

    /// The word the state row keeps for why the check could not run.
    fn reason_word(&self) -> Option<&'static str> {
        match self.found {
            WatchFound::CouldNotRun(why) => Some(why.word()),
            WatchFound::Ran { .. } => None,
        }
    }
}

/// One check applied to a watch: the state row to write, and the change row when the check was a change.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub next: WatchStateRow,
    /// The change to write: its kind, the check's exit code and its tail. None when the check was not a change.
    pub change: Option<(&'static str, Option<i64>, Option<String>)>,
    /// What Crew did or will do for the change: the action word stored on the change row, or an empty string when there is no row.
    pub action: &'static str,
    /// A failing change with `on_failing` = `template`: the caller tries the start first and records `needs_you` if it is
    /// refused.
    pub start_template: bool,
}

fn result_word(pass: bool) -> &'static str {
    if pass { "pass" } else { "fail" }
}

/// The state machine: one check applied to the state the watch had. Pure; the store write and the template start follow it.
/// `raised` says the failing state came from a failing change that put a Needs you row up, so a pass resolves that row; a
/// failing baseline, or a failure whose template started, put no row up and recovers with action `none`.
pub fn plan(
    previous: &WatchStateRow,
    check: &WatchCheck,
    on_failing_template: bool,
    raised: bool,
    at: i64,
) -> Planned {
    let mut next = previous.clone();
    next.last_checked_unix = Some(at);
    // The exit code is the last check that ran: a check that could not run has none and leaves it.
    if matches!(check.found, WatchFound::Ran { .. }) {
        next.last_exit_code = check.exit_code();
    }
    next.tail = check.tail.clone();
    match check.found {
        WatchFound::CouldNotRun(_) => {
            next.error_streak = previous.error_streak.saturating_add(1);
            next.last_state = "could_not_run".to_owned();
            next.last_reason = check.reason_word().map(str::to_owned);
            // The last result that ran is kept: a check that could not run tells nothing about it.
            Planned {
                next,
                change: None,
                action: "",
                start_template: false,
            }
        }
        WatchFound::Ran { pass, .. } => {
            next.error_streak = 0;
            next.last_reason = None;
            next.last_result = Some(result_word(pass).to_owned());
            next.last_state = if pass { "passing" } else { "failing" }.to_owned();
            let previous_pass = previous
                .last_result
                .as_deref()
                .and_then(|result| match result {
                    "pass" => Some(true),
                    "fail" => Some(false),
                    _ => None,
                });
            if !previous.baseline_done || previous_pass.is_none() {
                // The first check that runs: a baseline. It never raises, even when it fails.
                next.baseline_done = true;
                return Planned {
                    next,
                    change: Some(("baseline", check.exit_code(), check.tail.clone())),
                    action: "none",
                    start_template: false,
                };
            }
            if previous_pass == Some(pass) {
                // The same result again: the latest exit code and tail replace the last ones, and nothing is a change.
                return Planned {
                    next,
                    change: None,
                    action: "",
                    start_template: false,
                };
            }
            let action = if pass {
                if raised { "resolved" } else { "none" }
            } else if on_failing_template {
                "started_template"
            } else {
                "needs_you"
            };
            Planned {
                next,
                change: Some((
                    if pass { "passing" } else { "failing" },
                    check.exit_code(),
                    check.tail.clone(),
                )),
                action,
                start_template: !pass && on_failing_template,
            }
        }
    }
}

/// The tail a check keeps: its output redacted and cut to the last 20 lines and 2 KiB.
fn kept_tail(log: &str) -> Option<String> {
    if log.is_empty() {
        return None;
    }
    // Cut the raw log first, so redacting a command that printed megabytes is bounded; the kept 2 KiB is far inside it.
    let mut at = log.len().saturating_sub(RAW_KEEP);
    while at < log.len() && !log.is_char_boundary(at) {
        at += 1;
    }
    let redacted = redact(&log[at..]);
    let tail = checks::tail_lines_bytes(&redacted, TAIL_LINES, TAIL_BYTES);
    (!tail.is_empty()).then_some(tail)
}

/// The fixed sentence of a refused template start, by the code a change row kept. Core owns these words: no repository
/// text, crew name or command is ever part of them.
pub(crate) fn refusal_sentence(code: &str) -> &'static str {
    match code {
        "disk_low" => "Not started: not enough free space. Free space and try again.",
        "no_engine" => "No engine is set up for this template. Choose one in Settings.",
        "folder_untrusted" => "This folder is not trusted yet. Trust it in Settings.",
        "run_in_progress" => "A run of this crew is already running.",
        "not_allowed" => "Your settings do not allow this start.",
        "crew_gone" => "This watch no longer exists.",
        _ => "The start was refused. Open the run list for details.",
    }
}

fn change_wire(row: WatchChangeRow) -> Result<wire::BackgroundWatchChange> {
    Ok(wire::BackgroundWatchChange {
        id: row.id,
        kind: parse_wire(&row.kind)?,
        action: parse_wire(&row.action)?,
        reason: row
            .reason_code
            .as_deref()
            .map(|code| refusal_sentence(code).to_owned()),
        reason_code: row.reason_code,
        at: format_unix(row.at_unix),
        exit_code: row.exit_code.and_then(|code| i32::try_from(code).ok()),
        tail: row.tail,
    })
}

/// The state a watch that has never been checked reads as.
fn blank_state(crew: &str) -> WatchStateRow {
    WatchStateRow {
        background_id: crew.to_owned(),
        last_result: None,
        baseline_done: false,
        error_streak: 0,
        last_checked_unix: None,
        last_exit_code: None,
        last_state: "not_checked".to_owned(),
        last_reason: None,
        tail: None,
    }
}

fn status_wire(crew: &str, state: Option<WatchStateRow>) -> Result<wire::BackgroundWatchStatus> {
    let state = state.unwrap_or_else(|| blank_state(crew));
    Ok(wire::BackgroundWatchStatus {
        crew_id: state.background_id,
        state: parse_wire(&state.last_state)?,
        last_checked_at: state.last_checked_unix.map(format_unix),
        last_exit_code: state
            .last_exit_code
            .and_then(|code| i32::try_from(code).ok()),
        reason: state
            .last_reason
            .as_deref()
            .and_then(CouldNotRun::of_word)
            .map(|why| why.sentence().to_owned()),
        baseline_done: state.baseline_done,
        error_streak: state.error_streak,
        tail: state.tail,
    })
}

impl Core {
    /// Runs one watch's command once: screened again first, then run in `worktree` for at most `limit`. `limit` is the five
    /// minutes the tick passes; a test may shorten it. Nothing is written here and no model is called.
    pub(crate) async fn watch_check(
        &self,
        worktree: &Path,
        command: &str,
        limit: Duration,
    ) -> WatchCheck {
        if let Screened::Refused(_) = self.crew_screen_command(worktree, None, &[], command).await {
            return WatchCheck::could_not_run(CouldNotRun::Refused);
        }
        let deadline = Instant::now() + limit;
        let ran = checks::run_line_capped(command, worktree, deadline, OUTPUT_CAP).await;
        match ran.outcome {
            CheckOutcome::Pass | CheckOutcome::Fail => WatchCheck {
                found: WatchFound::Ran {
                    pass: ran.outcome == CheckOutcome::Pass,
                    exit_code: ran.exit_code,
                },
                tail: kept_tail(&ran.log),
            },
            _ => {
                let why = match ran.reason.as_deref() {
                    Some(RAN_OUT_OF_TIME) => CouldNotRun::TimedOut,
                    Some(TOO_MUCH_OUTPUT) => CouldNotRun::TooMuchOutput,
                    Some(reason) if reason.starts_with(COULD_NOT_START) => {
                        CouldNotRun::CouldNotStart
                    }
                    _ => CouldNotRun::CouldNotStart,
                };
                WatchCheck::could_not_run(why)
            }
        }
    }

    /// Applies one check to a watch: writes its state row and, when the check was a change, the change row, in one transaction.
    /// A turn to failing with `on_failing` = `template` first starts the crew's template through the listener seam; a start
    /// that is refused leaves the Needs you row raised. `event` names the firing in the start's `PausedTrigger` key
    /// (`watch:<bucket>`) and is never part of a prompt.
    pub(crate) async fn record_watch_check(
        self: &Arc<Self>,
        crew: &BackgroundRow,
        event: &str,
        check: WatchCheck,
        at: i64,
    ) -> Result<()> {
        let previous = self
            .store
            .watch_state(&crew.id)?
            .unwrap_or_else(|| blank_state(&crew.id));
        // What a failure does is read now, not from the row the tick loaded before a command that may have run for minutes: a
        // person who switched it to `needs_you` meanwhile gets a Needs you row, not a started template.
        let on_failing_template = self
            .background_row(&crew.id)
            .is_ok_and(|now| now.trigger_settings["on_failing"].as_str() == Some("template"));
        // A pass resolves only a row that was raised: the newest result change (a turn-off is history, not a result) must be the
        // failing change action `needs_you`. A failing baseline or a started template recovers with no action.
        let raised = self
            .store
            .latest_watch_result_change(&crew.id)?
            .is_some_and(|row| row.kind == "failing" && row.action == "needs_you");
        let planned = plan(&previous, &check, on_failing_template, raised, at);
        let mut action = planned.action;
        let mut reason_code = None;
        if planned.start_template {
            let fired = self.fire_background_event(&crew.id, "watch", event).await;
            if !fired.is_started() {
                action = "needs_you";
                reason_code = fired.refusal_code();
            }
        }
        let change = planned.change;
        let change = change
            .as_ref()
            .map(|(kind, exit_code, tail)| NewWatchChange {
                kind,
                action,
                reason_code,
                at_unix: at,
                exit_code: *exit_code,
                tail: tail.as_deref(),
            });
        self.store.save_watch_check(&planned.next, change.as_ref())
    }

    /// The person's read of a watch's last check. Behind the Crew Preview flag; a crew that is not a watch is refused plainly.
    pub fn background_watch(&self, id: &str) -> Result<wire::BackgroundWatchStatus> {
        self.crew_preview_only()?;
        valid_id(id)?;
        let crew = self.background_row(id)?;
        if crew.trigger_kind != "watch" {
            return Err(CoreError::Invalid(
                "This background crew is not a watch.".into(),
            ));
        }
        status_wire(&crew.id, self.store.watch_state(&crew.id)?)
    }

    /// One page of a watch's change history, newest first. Behind the Crew Preview flag.
    pub fn background_watch_changes(
        &self,
        id: &str,
        page: Page,
    ) -> Result<wire::BackgroundWatchChanges> {
        self.crew_preview_only()?;
        valid_id(id)?;
        let crew = self.background_row(id)?;
        if crew.trigger_kind != "watch" {
            return Err(CoreError::Invalid(
                "This background crew is not a watch.".into(),
            ));
        }
        let (rows, total) = self
            .store
            .watch_changes_page(&crew.id, page.limit, page.offset)?;
        Ok(wire::BackgroundWatchChanges {
            changes: rows.into_iter().map(change_wire).collect::<Result<_>>()?,
            total,
            next_offset: next_offset(page, total),
        })
    }
}

/// The event the watch tick fires a start with: `watch:<start>`. The tick passes it to `crew_background_start`, which builds
/// the held row's key as `<crew>:<event>` (so `<crew>:watch:<start>`); the claim is `watch:<crew>:<start>`.
pub fn event_key(start: i64) -> String {
    format!("watch:{start}")
}

/// The start, in unix seconds, of the `every_hours` window `now` falls in. The tick claims `watch:<crew>:<start>` and names its
/// event `watch:<start>`. A start is comparable across intervals, so a person who changes `every_hours` is never held back
/// by claims the old interval made: a window that starts after the newest claim checks, one that starts at or before it does not.
pub fn window_start(now: i64, hours: u32) -> i64 {
    let window = i64::from(hours) * 3600;
    now.div_euclid(window) * window
}
