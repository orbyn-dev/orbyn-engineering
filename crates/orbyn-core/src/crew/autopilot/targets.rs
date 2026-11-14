//! The blast-radius guard's two lists, on this device, and the test of what the guard says about a command (slice 4i, guard).
//!
//! **The lists never live in a repository.** They are a table in the service's data folder (Settings, Crew), so a file a worker or a
//! teammate commits cannot loosen the guard; `.orbyn/crew.json` is not read for it. Adding a target to `prod` or removing one from
//! `safe` only tightens the guard and needs nothing but the person's request. **Adding a `safe` target or removing a `prod` one
//! loosens it** and needs the same proof of presence as approving once: a grant the desktop app records after the person clicks in
//! its own dialog (see [`presence`](super::presence)). That dialog is not built yet, so for now loosening answers
//! `desktop_required` (no shell) or `presence_required` (a shell, and no grant can be minted) for every caller.

use orbyn_contracts as wire;
use orbyn_contracts::{ActionKind, GuardList};
use serde_json::json;

use super::blast::{Class, Facts, inspect, read_remotes};
use super::hard::{Scene, context};
use crate::app::Core;
use crate::crew::{find_secret, redact};
use crate::error::{CoreError, Result};
use crate::host::presence::PresenceFor;
use crate::store::GUARD_ENTRIES_MAX;
use crate::util::unix_now;

/// The longest pattern: a hostname's own limit.
pub const PATTERN_MAX: usize = 253;
/// The longest note, in characters.
pub const NOTE_MAX: usize = 200;
/// The longest command the test takes, in characters.
pub const TEST_COMMAND_MAX: usize = 2000;
/// The longest command the test shows back.
const SHOWN_MAX: usize = 400;

/// A pattern as it is stored: lowercase, one name or `*.suffix`, letters, digits, `.`, `_` and `-`. A URL, a user, a port, a path or
/// anything the secret scan objects to is refused, so no entry holds a credential.
fn pattern(text: &str) -> Result<String> {
    let text = text.trim().to_lowercase();
    if text.is_empty() || text.len() > PATTERN_MAX {
        return Err(CoreError::Invalid(format!(
            "a target is 1 to {PATTERN_MAX} characters"
        )));
    }
    if text.contains("://") || text.contains('@') || text.contains(':') || text.contains('/') {
        return Err(CoreError::Invalid(
            "a target is a bare name, not a URL, a user, a port or a path".into(),
        ));
    }
    let name = text.strip_prefix("*.").unwrap_or(&text);
    let clean = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..");
    if !clean {
        return Err(CoreError::Invalid(
            "a target is letters, digits, '.', '_' and '-', or '*.' and a name".into(),
        ));
    }
    if find_secret(&json!(text)).is_some() {
        return Err(CoreError::Invalid(
            "this looks like a secret; a target is a name".into(),
        ));
    }
    Ok(text)
}

/// A note as it is stored: 1 to [`NOTE_MAX`] characters, no control character, no secret. Empty is no note.
fn note(text: Option<&str>) -> Result<Option<String>> {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.chars().count() > NOTE_MAX {
        return Err(CoreError::Invalid(format!(
            "a note is at most {NOTE_MAX} characters"
        )));
    }
    if text.chars().any(char::is_control) {
        return Err(CoreError::Invalid(
            "a note has a character that cannot be shown".into(),
        ));
    }
    if find_secret(&json!(text)).is_some() {
        return Err(CoreError::Invalid(
            "this note holds a secret; it was not saved".into(),
        ));
    }
    Ok(Some(redact(text)))
}

fn presence_for(action: &str, pattern: &str) -> PresenceFor {
    PresenceFor::GuardLoosen {
        action: action.to_owned(),
        pattern: pattern.to_owned(),
    }
}

impl Core {
    /// Both lists.
    pub fn guard_targets(&self) -> Result<wire::GuardTargets> {
        self.crew_preview_only()?;
        Ok(wire::GuardTargets {
            items: self.store.crew_guard_targets()?,
            max_entries: GUARD_ENTRIES_MAX,
        })
    }

    /// Whether a person's grant covers this loosening. Asked first without using the grant, so a refusal later does not burn it.
    fn guard_loosening_confirmed(
        &self,
        action: &str,
        pattern: &str,
        nonce: Option<&str>,
        now: i64,
    ) -> Result<()> {
        if !self.shell.attached() {
            return Err(CoreError::DesktopRequired);
        }
        if !self
            .presence
            .confirmed_for(&presence_for(action, pattern), nonce, now)
        {
            return Err(CoreError::PresenceRequired);
        }
        Ok(())
    }

    /// Adds an entry. Tightening (`prod`) needs nothing more than the request; loosening (`safe`) needs the person's presence
    /// proof for exactly this pattern. The audit row is written in the same transaction, before the entry.
    pub fn guard_target_add(
        &self,
        body: &wire::GuardTargetAdd,
        nonce: Option<&str>,
    ) -> Result<wire::GuardTarget> {
        self.guard_target_add_at(body, nonce, unix_now())
    }

    pub(crate) fn guard_target_add_at(
        &self,
        body: &wire::GuardTargetAdd,
        nonce: Option<&str>,
        now: i64,
    ) -> Result<wire::GuardTarget> {
        self.crew_preview_only()?;
        let name = pattern(&body.pattern)?;
        let note = note(body.note.as_deref())?;
        let loosens = body.list == GuardList::Safe;
        if loosens {
            self.guard_loosening_confirmed("add_safe", &name, nonce, now)?;
        }
        // The grant is used up just before the write; a duplicate or a full list is caught by the write and leaves it spent, so the
        // person asks again.
        if loosens
            && !self
                .presence
                .take(&presence_for("add_safe", &name), nonce, now)
        {
            return Err(CoreError::PresenceRequired);
        }
        self.store
            .add_crew_guard_target(body.list, &name, body.kind, note.as_deref())
    }

    /// Removes an entry. Removing a `safe` entry tightens; removing a `prod` entry loosens and needs the presence proof.
    pub fn guard_target_remove(&self, id: &str, nonce: Option<&str>) -> Result<wire::GuardTarget> {
        self.guard_target_remove_at(id, nonce, unix_now())
    }

    pub(crate) fn guard_target_remove_at(
        &self,
        id: &str,
        nonce: Option<&str>,
        now: i64,
    ) -> Result<wire::GuardTarget> {
        self.crew_preview_only()?;
        super::merge::valid_id(id)?;
        let found = self
            .store
            .crew_guard_target(id)?
            .ok_or_else(|| CoreError::NotFound("guard target".into()))?;
        if found.list == GuardList::Prod {
            self.guard_loosening_confirmed("remove_prod", &found.pattern, nonce, now)?;
            if !self
                .presence
                .take(&presence_for("remove_prod", &found.pattern), nonce, now)
            {
                return Err(CoreError::PresenceRequired);
            }
        }
        self.store.remove_crew_guard_target(id)
    }

    /// What the guard says about a command: read-only. With a worker thread, a push is read against that worktree's remotes; the
    /// command is never run and never stored.
    pub async fn guard_test(&self, command: &str, thread: Option<&str>) -> Result<wire::GuardTest> {
        self.crew_preview_only()?;
        if command.trim().is_empty() || command.chars().count() > TEST_COMMAND_MAX {
            return Err(CoreError::Invalid(format!(
                "a command to test is 1 to {TEST_COMMAND_MAX} characters"
            )));
        }
        let worktree = match thread {
            Some(thread) => {
                super::merge::valid_id(thread)?;
                self.store
                    .crew_task_for_worker(thread)?
                    .and_then(|(_, run)| run.worktree)
                    .map(std::path::PathBuf::from)
            }
            None => None,
        };
        let pushes = command.contains("push");
        let remotes = match (&worktree, pushes) {
            (Some(tree), true) => read_remotes(tree).await,
            _ => std::collections::HashMap::new(),
        };
        let facts = Facts::new(self.store.crew_guard_targets()?, remotes);
        let tree = worktree.unwrap_or_default();
        let scene = Scene {
            worktree: &tree,
            protected: &[],
            allowed_hosts: &[],
            data_dir: None,
            branch: None,
            default_branch: None,
        };
        let ctx = context(ActionKind::Command, command, &[], &scene);
        let shown: String = redact(command.trim()).chars().take(SHOWN_MAX).collect();
        let Some(found) = inspect(&ctx, &facts) else {
            return Ok(wire::GuardTest {
                command: shown,
                verdict: wire::GuardVerdict::NotDestructive,
                category: None,
                matched_by: None,
                target: None,
                local: false,
                production_linked: false,
                matched_entry: None,
            });
        };
        let guarded = found.guarded();
        let exempt = found
            .targets
            .iter()
            .any(|t| matches!(t.class, Class::Safe(_)));
        Ok(wire::GuardTest {
            command: shown,
            verdict: if guarded {
                wire::GuardVerdict::Guarded
            } else if exempt {
                wire::GuardVerdict::Exempt
            } else {
                wire::GuardVerdict::Local
            },
            category: Some(found.category),
            matched_by: Some(redact(&found.matched_by)),
            target: Some(redact(&found.shown_target())),
            local: found.targets.iter().all(|t| t.class == Class::Local),
            production_linked: guarded,
            matched_entry: found.entry().cloned(),
        })
    }
}
