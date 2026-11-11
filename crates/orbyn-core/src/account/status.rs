//! Account status: what is true now, published to the windows over the shell event
//! stream as `DataChanged { what: "account" }` (and `account:conflicts`). A status
//! carries a kind and a safe reason, never a request, a body or a key.

use std::sync::OnceLock;

use tokio::sync::watch;

use crate::util::swr::Notify;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignOutReason {
    /// The session ended; key material is destroyed by the sign-out path, and the
    /// settings and the outbox stay.
    SessionEnded,
    AccountGone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseReason {
    ProtectionAdvanced,
    DeletionScheduled,
    InvalidKeyboardFile,
    /// The server refused the zone and said not to retry; other zones continue.
    ZoneFailed,
    /// The record is over its size limit and is not uploaded.
    TooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncState {
    SignedOut {
        reason: Option<SignOutReason>,
    },
    SigningIn,
    Online,
    Syncing,
    Offline {
        since_unix: i64,
    },
    /// Refused for now; no retry before the wait is over.
    Throttled {
        retry_after_secs: u64,
    },
    Paused {
        reason: PauseReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountStatus {
    pub state: SyncState,
    /// Fields waiting in the outbox.
    pub waiting: usize,
    /// Settings kept local because they are over a limit.
    pub not_synced: usize,
    /// Lost offline edits the person has not looked at yet.
    pub conflicts: usize,
}

impl Default for AccountStatus {
    fn default() -> Self {
        Self {
            state: SyncState::SignedOut { reason: None },
            waiting: 0,
            not_synced: 0,
            conflicts: 0,
        }
    }
}

/// Holds the current status and tells the windows when it changes.
pub struct StatusHub {
    tx: watch::Sender<AccountStatus>,
    notify: OnceLock<Notify>,
}

impl Default for StatusHub {
    fn default() -> Self {
        Self {
            tx: watch::channel(AccountStatus::default()).0,
            notify: OnceLock::new(),
        }
    }
}

impl StatusHub {
    pub fn set_notify(&self, notify: Notify) {
        let _ = self.notify.set(notify);
    }

    pub fn current(&self) -> AccountStatus {
        self.tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<AccountStatus> {
        self.tx.subscribe()
    }

    /// Publishes `next` when it differs, and says so on the event stream. A change in
    /// the conflict count also names `account:conflicts`.
    pub fn publish(&self, next: AccountStatus) {
        let previous = self.tx.borrow().clone();
        if previous == next {
            return;
        }
        let conflicts_changed = previous.conflicts != next.conflicts;
        self.tx.send_replace(next);
        if let Some(notify) = self.notify.get() {
            notify("account");
            if conflicts_changed {
                notify("account:conflicts");
            }
        }
    }

    pub fn update(&self, change: impl FnOnce(&mut AccountStatus)) {
        let mut next = self.current();
        change(&mut next);
        self.publish(next);
    }
}
