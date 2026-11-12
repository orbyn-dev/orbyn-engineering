//! A person's approval, given in the desktop app, kept in memory until the action it was for asks for it.
//!
//! The desktop shell shows a native dialog, and when the person clicks Approve it mints a one-time nonce and sends it to the
//! service on the channel only the shell holds (the service's stdin, see `shell`). The service records one grant per action here;
//! the page then sends the same nonce with the action's own request, and the action takes the grant, once. A caller that only
//! speaks HTTP to the service, with or without the service token, has no way to put a grant here.
//!
//! **Not OS-level presence.** What this proves is that the grant came in on the shell's channel and carries a nonce the page was
//! handed after it. A process of the same OS user can still drive the desktop app's native dialog, read the shell's memory or
//! write into its pipe (docs/crew.md, "Approving once: local presence").
//!
//! A grant is for exactly the fields it was made with, lives [`PRESENCE_LIFETIME_SECS`], is replaced by a later grant for the same
//! action, is used up by its first use and is never written to disk, so a restart drops every grant.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::util::same_secret;

/// How long a person's approval waits for the page to use it, in seconds.
pub const PRESENCE_LIFETIME_SECS: i64 = 60;
/// Grants kept at once; the oldest go first.
const KEPT: usize = 64;

/// What a person approved. Every field must match when the grant is used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresenceFor {
    /// One held command, once: the hold, the worker's thread, the task and the hold's binding hash.
    CrewApproveOnce {
        hold: String,
        thread: String,
        task: String,
        command_sha256: String,
    },
}

impl PresenceFor {
    /// The action a grant belongs to: a later grant for the same key replaces the earlier one.
    fn key(&self) -> &str {
        match self {
            Self::CrewApproveOnce { hold, .. } => hold,
        }
    }
}

struct Grant {
    nonce: String,
    what: PresenceFor,
    expires: i64,
}

#[derive(Default)]
pub struct PresenceBook {
    grants: Mutex<Vec<Grant>>,
}

fn lock(grants: &Mutex<Vec<Grant>>) -> MutexGuard<'_, Vec<Grant>> {
    grants.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether `text` is a nonce the shell may mint: 16 random bytes as lowercase hex.
pub fn is_nonce(text: &str) -> bool {
    text.len() == 32
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl PresenceBook {
    /// Records a grant, replacing the earlier one for the same action.
    pub fn put(&self, what: PresenceFor, nonce: String, now: i64) {
        let mut grants = lock(&self.grants);
        grants.retain(|g| g.expires > now && g.what.key() != what.key());
        grants.push(Grant {
            nonce,
            what,
            expires: now + PRESENCE_LIFETIME_SECS,
        });
        while grants.len() > KEPT {
            grants.remove(0);
        }
    }

    /// Whether a live grant for exactly this action carries this nonce. It does not use the grant up, so a refused request
    /// does not burn it.
    pub fn confirmed_for(&self, what: &PresenceFor, nonce: Option<&str>, now: i64) -> bool {
        let grants = lock(&self.grants);
        Self::matching(&grants, what, nonce, now).is_some()
    }

    /// Uses the grant up if it is live, for exactly this action, and carries this nonce.
    pub fn take(&self, what: &PresenceFor, nonce: Option<&str>, now: i64) -> bool {
        let mut grants = lock(&self.grants);
        let Some(at) = Self::matching(&grants, what, nonce, now) else {
            return false;
        };
        grants.remove(at);
        true
    }

    fn matching(
        grants: &[Grant],
        what: &PresenceFor,
        nonce: Option<&str>,
        now: i64,
    ) -> Option<usize> {
        let sent = nonce?;
        grants
            .iter()
            .position(|g| g.expires > now && g.what == *what && same_secret(&g.nonce, sent))
    }

    /// Grants held now (for tests).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        lock(&self.grants).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONCE: &str = "0123456789abcdef0123456789abcdef";

    fn what(hold: &str) -> PresenceFor {
        PresenceFor::CrewApproveOnce {
            hold: hold.into(),
            thread: "w-1".into(),
            task: "t-1".into(),
            command_sha256: "abc".into(),
        }
    }

    fn with(change: impl FnOnce(&mut String, &mut String, &mut String)) -> PresenceFor {
        let mut other = what("h-1");
        let PresenceFor::CrewApproveOnce {
            thread,
            task,
            command_sha256,
            ..
        } = &mut other;
        change(thread, task, command_sha256);
        other
    }

    #[test]
    fn a_grant_is_used_once_and_only_with_its_nonce() {
        let book = PresenceBook::default();
        book.put(what("h-1"), NONCE.into(), 100);
        assert!(!book.confirmed_for(&what("h-1"), None, 101));
        assert!(!book.confirmed_for(&what("h-1"), Some("0123456789abcdef0123456789abcdee"), 101));
        assert!(!book.take(&what("h-1"), Some("short"), 101));
        assert_eq!(book.len(), 1, "a wrong nonce does not burn the grant");
        assert!(book.confirmed_for(&what("h-1"), Some(NONCE), 101));
        assert!(
            book.confirmed_for(&what("h-1"), Some(NONCE), 102),
            "asking does not use it"
        );
        assert!(book.take(&what("h-1"), Some(NONCE), 102));
        assert!(!book.take(&what("h-1"), Some(NONCE), 103), "used up");
    }

    #[test]
    fn a_grant_matches_only_the_fields_it_was_made_with() {
        let book = PresenceBook::default();
        book.put(what("h-1"), NONCE.into(), 100);
        for other in [
            with(|thread, _, _| *thread = "w-2".into()),
            with(|_, task, _| *task = "t-2".into()),
            with(|_, _, hash| *hash = "changed".into()),
            what("h-2"),
        ] {
            assert!(!book.confirmed_for(&other, Some(NONCE), 101), "{other:?}");
        }
        assert!(book.confirmed_for(&what("h-1"), Some(NONCE), 101));
    }

    #[test]
    fn a_grant_expires_and_a_later_grant_replaces_the_earlier() {
        let book = PresenceBook::default();
        book.put(what("h-1"), NONCE.into(), 100);
        let live = 100 + PRESENCE_LIFETIME_SECS - 1;
        assert!(book.confirmed_for(&what("h-1"), Some(NONCE), live));
        assert!(!book.confirmed_for(&what("h-1"), Some(NONCE), live + 1));
        let second = "fedcba9876543210fedcba9876543210";
        book.put(what("h-1"), NONCE.into(), 200);
        book.put(what("h-1"), second.into(), 201);
        assert_eq!(book.len(), 1);
        assert!(
            !book.confirmed_for(&what("h-1"), Some(NONCE), 202),
            "the older one is gone"
        );
        assert!(book.confirmed_for(&what("h-1"), Some(second), 202));
    }

    #[test]
    fn grants_are_bounded_and_nonces_have_one_shape() {
        let book = PresenceBook::default();
        for n in 0..KEPT + 5 {
            book.put(what(&format!("h-{n}")), NONCE.into(), 100);
        }
        assert_eq!(book.len(), KEPT);
        assert!(is_nonce(NONCE));
        assert!(!is_nonce("0123456789ABCDEF0123456789ABCDEF"));
        assert!(!is_nonce("0123456789abcdef"));
        assert!(!is_nonce(&format!("{NONCE}0")));
    }
}
