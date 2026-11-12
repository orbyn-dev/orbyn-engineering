//! A command Autopilot held, and a person's one-time answer to it. **No agent approves; a person may approve once.**
//!
//! A chain denial of a command (the lead said no, or a second key is missing) leaves a hold. The worker is still denied and
//! goes on. A person may then, from the app's own window, approve that exact command once, deny it or dismiss it. An approval is
//! not a rule: it covers one retry of the same command, with the same arguments, from the same worker thread, within ten minutes,
//! and the retry is checked against the always-denied list again before it is let through. Nothing on that list is ever held, and
//! an approval cannot be made for it.
//!
//! The holds live in memory only, as the account's confirmations do: a restart ends the worker's turn and drops them, and the
//! raw command (which may hold a secret) is never written to disk. What is written is the ledger row of each decision, with the
//! redacted command and a hash of the exact one. Each hold is decided once, under one lock, so a racing approve and deny have
//! exactly one winner.
//!
//! Approving is the only answer that needs proof that a person is present: the account's "Confirm it's you" (the password again,
//! one-time nonce in `x-orbyn-confirm`, bound to this hold's id). See [`CONFIRM_PREFIX`] for what it does and does not protect.

use std::sync::{Mutex, MutexGuard, PoisonError};

use orbyn_contracts as wire;
use orbyn_contracts::ActionKind;
use serde_json::{Value, json};

use super::hard::{self, Scene};
use super::ledger;
use crate::account::ConfirmFor;
use crate::app::Core;
use crate::crew::lifecycle::is_final;
use crate::crew::{key_needles, wire_name};
use crate::error::{CoreError, Result};
use crate::governance::approvals::ActionAsk;
use crate::store::{CrewRun, CrewTask};
use crate::util::{new_id, sha256_hex, unix_now};

/// How long a hold waits for the person, in seconds.
pub const HELD_LIFETIME_SECS: i64 = 10 * 60;
/// How long an approval waits for the worker's retry, in seconds.
pub const APPROVAL_LIFETIME_SECS: i64 = 10 * 60;
/// The longest redacted command a person is asked to approve. A longer one is held but cannot be approved once: a person must
/// be able to read all of what they approve.
pub const SHOWN_MAX: usize = 1000;
/// Open holds per task. Past this a worker's further denials are not held.
const OPEN_PER_TASK: usize = 20;
/// Holds kept in all, open or decided; the oldest decided ones go first.
const KEPT: usize = 200;

/// The prefix of the id a hold's confirmation is made for. The window asks `POST /api/account/confirm` with the action
/// `approve_device` and the `device_id` `crew-approve-once:<hold id>`, and sends the nonce it gets back as `x-orbyn-confirm`.
///
/// **What it protects:** the one-time nonce comes only from a password check, so a caller that holds nothing but the service
/// token (a worker, per #441) cannot approve; it is for one hold, used up by its first use, ten minutes at most, in memory only.
/// **What it does not:** it needs a signed-in account (without one, nothing can be approved once), and a worker that can read
/// the password or drive the window (the sandbox, #441) can still defeat it. Deny and dismiss need no password, so a caller
/// with the token can deny or dismiss a hold (never approve one).
///
/// **In release builds approve-once always fails** (`account_disabled`: the account is off). **In dev builds the stand-in
/// account's password is public**, so a worker holding the token (#441) could call `/api/account/confirm` itself, by a script N1
/// does not read, and approve its own hold: the protection is only as strong as the account password. As a code guard,
/// approve-once is refused while a seeded stand-in account (an `@example.com` address on the development backend) is signed in.
/// A dedicated confirmation action replaces this id convention once the account's own code can change.
pub const CONFIRM_PREFIX: &str = "crew-approve-once:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Held,
    Approved,
    Denied,
    Dismissed,
    Used,
    Expired,
}

impl Stage {
    fn wire(self) -> wire::AutopilotHeldState {
        match self {
            Self::Held => wire::AutopilotHeldState::Held,
            Self::Approved => wire::AutopilotHeldState::Approved,
            Self::Denied => wire::AutopilotHeldState::Denied,
            Self::Dismissed => wire::AutopilotHeldState::Dismissed,
            Self::Used => wire::AutopilotHeldState::Used,
            Self::Expired => wire::AutopilotHeldState::Expired,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Dismissed => "dismissed",
            Self::Used => "used",
            Self::Expired => "expired",
        }
    }
}

// No `Debug` outside tests: `ask` holds the raw command, which may hold a secret.
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub(super) struct Held {
    pub id: String,
    pub task_id: String,
    pub crew_id: String,
    pub thread_id: String,
    /// The hash of the exact command, its arguments and the thread.
    pub bound: String,
    pub kind: String,
    pub shown: String,
    pub readable: bool,
    pub reason: String,
    pub held_at: i64,
    /// When the hold stops waiting for a decision.
    pub deadline: i64,
    /// When an approval stops waiting for its retry; 0 until approved.
    pub until: i64,
    pub stage: Stage,
    /// The exact request, only while it is held: it is dropped when the hold is decided or ends.
    pub ask: Option<ActionAsk>,
}

/// What a new hold is made from.
pub(super) struct NewHold {
    pub task_id: String,
    pub crew_id: String,
    pub thread_id: String,
    pub ask: ActionAsk,
    pub shown: String,
    pub readable: bool,
    pub reason: String,
}

/// Why a decision did not happen.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failure {
    Unknown,
    /// Its time ran out.
    Over,
    /// It was decided already, as this.
    Decided(Stage),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Added {
    New(String),
    /// The same request is already held.
    Existing(String),
    /// The person denied this exact request lately: it is not held again.
    Denied,
    /// This task has too many open holds.
    Full,
}

#[derive(Default)]
pub struct HeldBook {
    items: Vec<Held>,
}

/// The hash a hold and its retry are matched by: the task, the worker's thread, the kind, the whole command text, every path and
/// the details, in a form that does not depend on key order.
pub(super) fn bind(task_id: &str, thread_id: &str, ask: &ActionAsk) -> String {
    let document = json!([
        task_id,
        thread_id,
        wire_name(&ask.kind),
        ask.target,
        ask.paths,
        canonical(&ask.detail)
    ]);
    sha256_hex(document.to_string().as_bytes())
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut pairs: Vec<_> = map.iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(b.0));
            Value::Array(
                pairs
                    .into_iter()
                    .map(|(key, item)| json!([key, canonical(item)]))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

impl HeldBook {
    /// Ends what ran out of time and says which holds that was.
    pub(super) fn sweep(&mut self, now: i64) -> Vec<Held> {
        let mut ended = Vec::new();
        for item in &mut self.items {
            let over = match item.stage {
                Stage::Held => now >= item.deadline,
                Stage::Approved => now >= item.until,
                _ => false,
            };
            if over {
                item.stage = Stage::Expired;
                item.ask = None;
                ended.push(item.clone());
            }
        }
        ended
    }

    pub(super) fn add(&mut self, new: NewHold, now: i64) -> Added {
        let bound = bind(&new.task_id, &new.thread_id, &new.ask);
        for item in &self.items {
            if item.bound == bound && item.deadline > now {
                match item.stage {
                    Stage::Held | Stage::Approved => return Added::Existing(item.id.clone()),
                    Stage::Denied => return Added::Denied,
                    _ => {}
                }
            }
        }
        let open = self
            .items
            .iter()
            .filter(|i| i.task_id == new.task_id && i.stage == Stage::Held)
            .count();
        if open >= OPEN_PER_TASK {
            return Added::Full;
        }
        let id = format!("hold-{}", new_id());
        self.items.push(Held {
            id: id.clone(),
            task_id: new.task_id,
            crew_id: new.crew_id,
            thread_id: new.thread_id,
            bound,
            kind: wire_name(&new.ask.kind),
            shown: new.shown,
            readable: new.readable,
            reason: new.reason,
            held_at: now,
            deadline: now + HELD_LIFETIME_SECS,
            until: 0,
            stage: Stage::Held,
            ask: Some(new.ask),
        });
        // Past the limit the oldest finished holds go. An approval waiting for its retry, an open hold and a denial still in
        // force (it keeps the same command from being held again) stay until they end.
        while self.items.len() > KEPT {
            let gone = |i: &Held| match i.stage {
                Stage::Used | Stage::Expired | Stage::Dismissed => true,
                Stage::Denied => i.deadline <= now,
                Stage::Held | Stage::Approved => false,
            };
            match self.items.iter().position(gone) {
                Some(at) => {
                    self.items.remove(at);
                }
                None => break,
            }
        }
        Added::New(id)
    }

    pub(super) fn get(&self, id: &str) -> Option<&Held> {
        self.items.iter().find(|item| item.id == id)
    }

    /// One hold that can still be decided. The caller has swept first.
    pub(super) fn open(&self, id: &str) -> std::result::Result<Held, Failure> {
        match self.get(id) {
            None => Err(Failure::Unknown),
            Some(item) if item.stage == Stage::Held => Ok(item.clone()),
            Some(item) if item.stage == Stage::Expired => Err(Failure::Over),
            Some(item) => Err(Failure::Decided(item.stage)),
        }
    }

    /// The one decision. Only a held command can be decided, and only once.
    pub(super) fn decide(
        &mut self,
        id: &str,
        now: i64,
        to: Stage,
    ) -> std::result::Result<Held, Failure> {
        let item = self.items.iter_mut().find(|i| i.id == id);
        let Some(item) = item else {
            return Err(Failure::Unknown);
        };
        match item.stage {
            Stage::Held if now < item.deadline => {}
            Stage::Held | Stage::Expired => return Err(Failure::Over),
            other => return Err(Failure::Decided(other)),
        }
        item.stage = to;
        item.ask = None;
        if to == Stage::Approved {
            item.until = now + APPROVAL_LIFETIME_SECS;
        }
        Ok(item.clone())
    }

    /// Ends one hold without a decision (its task ended, or the approval-time check refused).
    pub(super) fn close(&mut self, id: &str, to: Stage) -> Option<Held> {
        let item = self.items.iter_mut().find(|i| i.id == id)?;
        if item.stage != Stage::Held {
            return None;
        }
        item.stage = to;
        item.ask = None;
        Some(item.clone())
    }

    /// Uses the approval for exactly this request, once. Nothing else matches: not another command, other arguments, another
    /// thread, another task, a used or an expired approval.
    pub(super) fn take(
        &mut self,
        task_id: &str,
        thread_id: &str,
        bound: &str,
        now: i64,
    ) -> Option<Held> {
        let item = self.items.iter_mut().find(|i| {
            i.stage == Stage::Approved
                && i.until > now
                && i.bound == bound
                && i.task_id == task_id
                && i.thread_id == thread_id
        })?;
        item.stage = Stage::Used;
        Some(item.clone())
    }

    fn view(item: &Held, now: i64) -> wire::AutopilotHeld {
        let left = match item.stage {
            Stage::Held => item.deadline - now,
            Stage::Approved => item.until - now,
            _ => 0,
        };
        wire::AutopilotHeld {
            id: item.id.clone(),
            task_id: item.task_id.clone(),
            crew_id: item.crew_id.clone(),
            kind: item.kind.clone(),
            command: item.shown.clone(),
            reason: item.reason.clone(),
            state: item.stage.wire(),
            readable: item.readable,
            age_secs: u32::try_from((now - item.held_at).max(0)).unwrap_or(u32::MAX),
            expires_in_secs: u32::try_from(left.max(0)).unwrap_or(u32::MAX),
        }
    }
}

fn lock(book: &Mutex<HeldBook>) -> MutexGuard<'_, HeldBook> {
    book.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How a person decides a held command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Deny,
    Dismiss,
}

impl Core {
    fn held_book(&self) -> MutexGuard<'_, HeldBook> {
        lock(&self.crew_autopilot.held)
    }

    fn held_needles(&self, task_id: &str) -> Vec<String> {
        self.store
            .crew_answer_key(task_id)
            .ok()
            .flatten()
            .map(|key| key_needles(&key))
            .unwrap_or_default()
    }

    /// One ledger row for a hold. A row that cannot be written is logged: the in-memory decision stands, as the hold itself is
    /// not a record.
    fn held_row(&self, held: &Held, kind: &str, extra: &Value) {
        let needles = self.held_needles(&held.task_id);
        let mut body = json!({
            "hold": held.id,
            // A deny or a dismiss has no proof of who sent it, so it says only that the call came from a local caller.
            "decided_by": match kind {
                "held" | "hold_ended" | "hold_used" => "autopilot",
                "denied_by_person" | "dismissed_by_person" => "local_caller",
                _ => "person",
            },
            "bound": format!("sha256:{}", held.bound),
            "action": { "kind": held.kind, "target": held.shown, "paths": [] },
        });
        if let (Some(into), Some(from)) = (body.as_object_mut(), extra.as_object()) {
            into.extend(from.clone());
        }
        if let Err(error) = self.store.add_crew_autopilot_event(
            &held.task_id,
            &held.crew_id,
            kind,
            &ledger::safe(&body, &needles),
        ) {
            tracing::warn!(%error, "autopilot could not write a ledger row for a held command");
        }
    }

    fn held_ended(&self, ended: Vec<Held>) {
        for held in ended {
            self.held_row(&held, "hold_ended", &json!({ "why": "expired" }));
        }
    }

    /// Holds a chain-denied command for the person, unless it is not a command, was denied by the person a moment ago or this
    /// task holds too many. Returns the hold's id.
    pub(crate) fn held_make(
        &self,
        task: &CrewTask,
        thread_id: &str,
        ask: &ActionAsk,
        reason: &str,
        needles: &[String],
        now: i64,
    ) -> Option<String> {
        if ask.kind != ActionKind::Command {
            return None;
        }
        let redacted = crate::crew::withhold(&crate::crew::redact(&ask.target), needles);
        let readable = redacted.chars().count() <= SHOWN_MAX;
        let mut shown: String = redacted.chars().take(SHOWN_MAX).collect();
        // The same last check as a ledger row: what is shown holds no secret and no key text, or nothing is shown and nothing can
        // be approved.
        let shown_value = json!(shown);
        let readable = readable
            && crate::crew::refuse_secret(&shown_value).is_ok()
            && crate::crew::refuse_leak(&shown_value, needles).is_ok();
        if !readable && !shown.is_empty() && crate::crew::refuse_secret(&shown_value).is_err() {
            shown = "No command can be shown.".to_owned();
        }
        let new = NewHold {
            task_id: task.id.clone(),
            crew_id: task.crew_id.clone(),
            thread_id: thread_id.to_owned(),
            ask: ask.clone(),
            shown,
            readable,
            reason: ledger::shown(reason, needles),
        };
        let (added, ended) = {
            let mut book = self.held_book();
            let ended = book.sweep(now);
            (book.add(new, now), ended)
        };
        self.held_ended(ended);
        match added {
            Added::New(id) => {
                let held = self.held_book().get(&id).cloned()?;
                self.held_row(
                    &held,
                    "held",
                    &json!({ "reason": held.reason, "readable": held.readable, "expires_in_secs": HELD_LIFETIME_SECS }),
                );
                Some(id)
            }
            Added::Existing(id) => Some(id),
            Added::Denied | Added::Full => None,
        }
    }

    /// Uses the person's approval for this exact request, if there is a live one. The caller has already checked the
    /// always-denied list for this request.
    pub(super) fn held_take(
        &self,
        task: &CrewTask,
        thread_id: &str,
        ask: &ActionAsk,
        now: i64,
    ) -> Option<String> {
        let bound = bind(&task.id, thread_id, ask);
        let (taken, ended) = {
            let mut book = self.held_book();
            let ended = book.sweep(now);
            (book.take(&task.id, thread_id, &bound, now), ended)
        };
        self.held_ended(ended);
        let held = taken?;
        self.held_row(&held, "hold_used", &json!({ "used": true }));
        Some(held.id)
    }

    /// Holds still open or decided lately, newest first. A hold whose task has ended is closed here.
    pub fn autopilot_held(&self) -> Result<wire::AutopilotHeldList> {
        self.autopilot_held_at(unix_now())
    }

    pub(crate) fn autopilot_held_at(&self, now: i64) -> Result<wire::AutopilotHeldList> {
        self.crew_preview_only()?;
        self.held_sweep(now);
        let items = self
            .held_book()
            .items
            .iter()
            .rev()
            .map(|i| HeldBook::view(i, now))
            .collect();
        Ok(wire::AutopilotHeldList {
            items,
            hold_secs: u32::try_from(HELD_LIFETIME_SECS).unwrap_or(0),
            approval_secs: u32::try_from(APPROVAL_LIFETIME_SECS).unwrap_or(0),
        })
    }

    /// Ends what ran out of time, and every open hold or approval whose worker's task is over.
    fn held_sweep(&self, now: i64) {
        let ended = self.held_book().sweep(now);
        self.held_ended(ended);
        let open: Vec<(String, String)> = self
            .held_book()
            .items
            .iter()
            .filter(|i| matches!(i.stage, Stage::Held | Stage::Approved))
            .map(|i| (i.id.clone(), i.thread_id.clone()))
            .collect();
        for (id, thread) in open {
            if self.live_worker(&thread).is_none() {
                self.held_ended_by_task(&id);
            }
        }
    }

    fn held_ended_by_task(&self, id: &str) {
        let closed = {
            let mut book = self.held_book();
            let Some(item) = book.items.iter_mut().find(|i| i.id == id) else {
                return;
            };
            if !matches!(item.stage, Stage::Held | Stage::Approved) {
                return;
            }
            item.stage = Stage::Expired;
            item.ask = None;
            item.clone()
        };
        self.held_row(&closed, "hold_ended", &json!({ "why": "task_ended" }));
    }

    /// The task and run of a worker thread that is still running a task.
    fn live_worker(&self, thread_id: &str) -> Option<(CrewTask, CrewRun)> {
        let (task, run) = self.store.crew_task_for_worker(thread_id).ok().flatten()?;
        (!is_final(&task.state)).then_some((task, run))
    }

    fn held_failure(failure: Failure) -> CoreError {
        match failure {
            Failure::Unknown => CoreError::NotFound("held command".into()),
            Failure::Over => CoreError::Gone(
                "This command is no longer held: its time ran out or its task ended.".into(),
            ),
            Failure::Decided(stage) => CoreError::InvalidTransition {
                from: stage.name().into(),
                to: "decided".into(),
            },
        }
    }

    /// Approves one held command, once. Only a person's action: the call needs the nonce that "Confirm it's you" made for this
    /// hold (see [`CONFIRM_PREFIX`]), and nothing else in the core calls it. It checks, in this order: the confirmation (without
    /// using it), that the hold is open, that its task is still running, that the command can be read whole, and the
    /// always-denied list again, now; then uses the confirmation up and decides, once.
    pub async fn autopilot_approve_once(
        &self,
        id: &str,
        nonce: Option<&str>,
    ) -> Result<wire::AutopilotHeld> {
        self.autopilot_approve_once_at(id, nonce, unix_now()).await
    }

    pub(crate) async fn autopilot_approve_once_at(
        &self,
        id: &str,
        nonce: Option<&str>,
        now: i64,
    ) -> Result<wire::AutopilotHeld> {
        self.crew_preview_only()?;
        super::merge::valid_id(id)?;
        let action = ConfirmFor::ApproveDevice(format!("{CONFIRM_PREFIX}{id}"));
        if !self.account.available() {
            return Err(CoreError::AccountDisabled);
        }
        if self.stand_in_account_signed_in().await {
            return Err(CoreError::PolicyDenied(
                "Approving once is not available while the development stand-in account is signed in: its password is public."
                    .into(),
            ));
        }
        if !self.account.confirmed_for(&action, nonce) {
            return Err(CoreError::ReauthRequired);
        }
        self.held_sweep(now);
        let held = self.held_book().open(id).map_err(Self::held_failure)?;
        let Some((task, run)) = self
            .live_worker(&held.thread_id)
            .filter(|(t, _)| t.id == held.task_id)
        else {
            self.held_ended_by_task(id);
            return Err(Self::held_failure(Failure::Over));
        };
        if !held.readable {
            return Err(CoreError::Invalid(
                "This command is too long to read whole, so it cannot be approved once.".into(),
            ));
        }
        if let Some(refusal) = self.refuse_now(&task, &run, &held).await? {
            let closed = self.held_book().close(id, Stage::Denied);
            if let Some(closed) = closed {
                self.held_row(
                    &closed,
                    "denied_by_person",
                    &json!({ "decided_by": "hard_rule", "rule": refusal.0, "name": refusal.1, "why": "always_denied_at_approval" }),
                );
            }
            return Err(CoreError::PolicyDenied(format!(
                "Denied by Autopilot ({}): {}. No approval can change this.",
                refusal.0, refusal.1
            )));
        }
        // The confirmation is used up here, then the one decision is made: a racing deny or a second approve loses.
        self.account.require_confirmed(action, nonce).await?;
        let decided = self.held_book().decide(id, now, Stage::Approved);
        let held = decided.map_err(Self::held_failure)?;
        self.held_row(
            &held,
            "approved_once",
            &json!({ "via": "local_session", "expires_in_secs": APPROVAL_LIFETIME_SECS }),
        );
        Ok(HeldBook::view(&held, now))
    }

    /// The development backend, signed in as one of its seeded `@example.com` accounts, whose password is public.
    async fn stand_in_account_signed_in(&self) -> bool {
        #[cfg(any(test, debug_assertions))]
        {
            if self.config.account_backend != crate::account::AccountBackendKind::Development {
                return false;
            }
            self.account
                .view(std::env::consts::OS)
                .await
                .session
                .is_some_and(|session| session.email.to_ascii_lowercase().ends_with("@example.com"))
        }
        #[cfg(not(any(test, debug_assertions)))]
        {
            false
        }
    }

    /// The always-denied list, against the exact request, as it stands now.
    async fn refuse_now(
        &self,
        task: &CrewTask,
        run: &CrewRun,
        held: &Held,
    ) -> Result<Option<(&'static str, &'static str)>> {
        let (Some(ask), Some(key), Some(worktree)) = (
            held.ask.as_ref(),
            self.store.crew_answer_key(&task.id)?,
            run.worktree.as_deref().map(std::path::PathBuf::from),
        ) else {
            return Err(CoreError::Gone(
                "This command can no longer be checked.".into(),
            ));
        };
        let ground = super::review::Ground::read(self, &worktree, run.base_commit.as_deref()).await;
        let scene: Scene<'_> = ground.scene(&key.protected_paths);
        Ok(hard::refuse(ask.kind, &ask.target, &ask.paths, &scene).map(|r| (r.id, r.rule)))
    }

    /// Denies a held command: the worker is not told to retry, and the same command is not held again for the rest of the hold's
    /// time. No password: it only ever refuses.
    pub fn autopilot_deny_held(&self, id: &str) -> Result<wire::AutopilotHeld> {
        self.decide_held(id, Decision::Deny, unix_now())
    }

    /// Dismisses a held command from the list without a decision. A retry may be held again.
    pub fn autopilot_dismiss_held(&self, id: &str) -> Result<wire::AutopilotHeld> {
        self.decide_held(id, Decision::Dismiss, unix_now())
    }

    pub(crate) fn decide_held(
        &self,
        id: &str,
        how: Decision,
        now: i64,
    ) -> Result<wire::AutopilotHeld> {
        self.crew_preview_only()?;
        super::merge::valid_id(id)?;
        self.held_sweep(now);
        let (stage, kind) = match how {
            Decision::Deny => (Stage::Denied, "denied_by_person"),
            Decision::Dismiss => (Stage::Dismissed, "dismissed_by_person"),
        };
        let held = self
            .held_book()
            .decide(id, now, stage)
            .map_err(Self::held_failure)?;
        self.held_row(&held, kind, &json!({ "via": "local_session" }));
        Ok(HeldBook::view(&held, now))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn ask(command: &str) -> ActionAsk {
        ActionAsk {
            kind: ActionKind::Command,
            target: command.to_owned(),
            summary: String::new(),
            detail: json!({ "cwd": "/w", "args": ["a"] }),
            paths: Vec::new(),
        }
    }

    fn new_hold(command: &str, thread: &str) -> NewHold {
        NewHold {
            task_id: "t-1".into(),
            crew_id: "c-1".into(),
            thread_id: thread.into(),
            ask: ask(command),
            shown: command.into(),
            readable: true,
            reason: "no".into(),
        }
    }

    fn held(book: &mut HeldBook, command: &str) -> String {
        match book.add(new_hold(command, "w-1"), 100) {
            Added::New(id) => id,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_hash_binds_command_arguments_thread_and_task_and_ignores_key_order() {
        let base = bind("t-1", "w-1", &ask("make deploy"));
        assert_eq!(base, bind("t-1", "w-1", &ask("make deploy")));
        assert_ne!(base, bind("t-1", "w-1", &ask("make deploy ")));
        assert_ne!(base, bind("t-1", "w-2", &ask("make deploy")));
        assert_ne!(base, bind("t-2", "w-1", &ask("make deploy")));
        let mut other_args = ask("make deploy");
        other_args.detail = json!({ "cwd": "/w", "args": ["b"] });
        assert_ne!(base, bind("t-1", "w-1", &other_args));
        let mut reordered = ask("make deploy");
        reordered.detail = json!({ "args": ["a"], "cwd": "/w" });
        assert_eq!(base, bind("t-1", "w-1", &reordered));
        let mut with_path = ask("make deploy");
        with_path.paths = vec!["x".into()];
        assert_ne!(base, bind("t-1", "w-1", &with_path));
    }

    #[test]
    fn an_approval_is_used_once_and_only_by_the_same_request() {
        let mut book = HeldBook::default();
        let id = held(&mut book, "make deploy");
        let bound = bind("t-1", "w-1", &ask("make deploy"));
        assert!(
            book.take("t-1", "w-1", &bound, 101).is_none(),
            "not approved yet"
        );
        book.decide(&id, 110, Stage::Approved).unwrap();
        assert!(
            book.get(&id).unwrap().ask.is_none(),
            "the raw request is dropped"
        );
        let other = bind("t-1", "w-1", &ask("make deploy now"));
        assert!(book.take("t-1", "w-1", &other, 111).is_none());
        assert!(
            book.take("t-1", "w-2", &bound, 111).is_none(),
            "another thread"
        );
        assert!(
            book.take("t-2", "w-1", &bound, 111).is_none(),
            "another task"
        );
        assert_eq!(book.take("t-1", "w-1", &bound, 112).unwrap().id, id);
        assert!(book.take("t-1", "w-1", &bound, 113).is_none(), "used up");
        assert_eq!(book.get(&id).unwrap().stage, Stage::Used);
    }

    #[test]
    fn a_hold_and_an_approval_expire() {
        let mut book = HeldBook::default();
        let id = held(&mut book, "a");
        assert_eq!(book.sweep(100 + HELD_LIFETIME_SECS - 1).len(), 0);
        assert_eq!(book.sweep(100 + HELD_LIFETIME_SECS).len(), 1);
        assert_eq!(
            book.decide(&id, 100 + HELD_LIFETIME_SECS, Stage::Approved)
                .unwrap_err(),
            Failure::Over
        );
        let id = held(&mut book, "b");
        book.decide(&id, 200, Stage::Approved).unwrap();
        let bound = bind("t-1", "w-1", &ask("b"));
        assert!(
            book.take("t-1", "w-1", &bound, 200 + APPROVAL_LIFETIME_SECS)
                .is_none()
        );
    }

    #[test]
    fn a_hold_is_decided_once_and_a_denial_stands() {
        let mut book = HeldBook::default();
        let id = held(&mut book, "a");
        book.decide(&id, 101, Stage::Denied).unwrap();
        assert_eq!(
            book.decide(&id, 102, Stage::Approved).unwrap_err(),
            Failure::Decided(Stage::Denied)
        );
        assert_eq!(book.add(new_hold("a", "w-1"), 103), Added::Denied);
        assert_eq!(
            book.decide("nope", 102, Stage::Denied).unwrap_err(),
            Failure::Unknown
        );
    }

    #[test]
    fn the_same_request_is_one_hold_and_a_task_holds_a_bounded_number() {
        let mut book = HeldBook::default();
        let id = held(&mut book, "a");
        assert_eq!(book.add(new_hold("a", "w-1"), 101), Added::Existing(id));
        for n in 0..OPEN_PER_TASK - 1 {
            held(&mut book, &format!("c{n}"));
        }
        assert_eq!(book.add(new_hold("zzz", "w-1"), 102), Added::Full);
    }

    #[test]
    fn eviction_keeps_an_approval_an_open_hold_and_a_denial_in_force() {
        let mut book = HeldBook::default();
        let approved = held(&mut book, "approved");
        book.decide(&approved, 101, Stage::Approved).unwrap();
        let denied = held(&mut book, "denied");
        book.decide(&denied, 101, Stage::Denied).unwrap();
        for n in 0..KEPT + 10 {
            let mut hold = new_hold(&format!("n{n}"), "w-1");
            hold.task_id = format!("t-{n}");
            if let Added::New(id) = book.add(hold, 102) {
                book.decide(&id, 103, Stage::Dismissed).unwrap();
            }
        }
        assert!(book.items.len() <= KEPT + 1);
        assert_eq!(book.get(&approved).unwrap().stage, Stage::Approved);
        assert_eq!(book.get(&denied).unwrap().stage, Stage::Denied);
    }

    #[test]
    fn a_racing_approve_and_deny_have_exactly_one_winner() {
        for _ in 0..50 {
            let book = Arc::new(Mutex::new(HeldBook::default()));
            let id = held(&mut lock(&book), "a");
            let mut threads = Vec::new();
            for to in [
                Stage::Approved,
                Stage::Denied,
                Stage::Approved,
                Stage::Dismissed,
            ] {
                let (book, id) = (book.clone(), id.clone());
                threads.push(std::thread::spawn(move || {
                    lock(&book).decide(&id, 101, to).is_ok()
                }));
            }
            let wins = threads
                .into_iter()
                .map(|t| t.join().unwrap())
                .filter(|won| *won)
                .count();
            assert_eq!(wins, 1);
        }
    }
}
