//! The app's side of the Engine's `DeviceApi`: push and pull per zone, and status.
//! It is shaped like the contract in [docs/account.md](../../../../docs/account.md#dependency-on-the-engine)
//! so the real `orbyn-device-client` can replace it, and it names no Engine crate: tag
//! `engine-v0.1.0` does not exist yet. The client does no sealing here; a record
//! crosses this trait as plain fields.

use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::watch;

use super::merge::Record;

/// A zone's version, bumped by the server on every accepted upload.
pub type Version = u64;

/// A zone's record at one version.
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneSnapshot {
    pub version: Version,
    pub record: Record,
}

/// The client's errors, with the contract's names. None carries a request, a header
/// or a key, so formatting one can leak nothing.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum DeviceError {
    #[error("offline")]
    Offline,
    #[error("session ended")]
    Unauthorized,
    #[error("invite required")]
    InviteRequired,
    #[error("wrong password")]
    WrongPassword,
    #[error("rate limited")]
    RateLimited { retry_after: Duration },
    /// The base version was stale: the zone's current record, to merge and upload again.
    #[error("conflict")]
    Conflict(ZoneSnapshot),
    #[error("too large")]
    TooLarge,
    #[error("invalid key")]
    InvalidKey,
    #[error("server error")]
    Server { retryable: bool },
    #[error("invalid code")]
    InvalidCode,
    #[error("deletion scheduled")]
    DeletionScheduled,
    #[error("protection advanced")]
    ProtectionAdvanced,
    #[error("account gone")]
    AccountGone,
}

/// What the client reports about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceStatus {
    SignedOut,
    SigningIn,
    Online,
    Offline { since_unix: i64 },
    Paused { reason: String },
}

#[async_trait]
pub trait DeviceApi: Send + Sync {
    /// The zone's current record, or `None` when the account has none yet.
    async fn pull(&self, zone: &str) -> Result<Option<ZoneSnapshot>, DeviceError>;

    /// Uploads the whole merged record. `base` is the version it was merged on, or
    /// `None` for a zone this device has never seen. A stale base is a
    /// [`DeviceError::Conflict`] carrying the current record.
    async fn push(
        &self,
        zone: &str,
        base: Option<Version>,
        record: &Record,
    ) -> Result<Version, DeviceError>;

    fn status(&self) -> watch::Receiver<DeviceStatus>;

    /// Counts live notices that another device changed a zone.
    fn notices(&self) -> watch::Receiver<u64>;
}
