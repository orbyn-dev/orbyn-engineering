//! The Engine's snapshot on disk, sealed under the device's vault key with XChaCha20-Poly1305.
//! The snapshot holds the zone keys, so the file is as secret as the keys.
//!
//! File: `version (1 byte) | counter (8 bytes, big-endian) | nonce (24) | ciphertext`. The AEAD's
//! associated data binds the purpose, the profile id, the format version and the counter, so a
//! file cannot move to another profile, be taken for another kind of file, or have its version
//! or counter edited. The vault holds the counter and the SHA-256 of the newest file; a file
//! that is older (lower counter) or does not match is refused, because an engine can put an
//! older sealed file back and the snapshot carries values that only move forward (docs/account-
//! threat-model.md, control 1).
//!
//! A refused or unreadable file loads as nothing: the Engine then starts empty and the device
//! learns its zones again. Nothing in a refusal, a log line or `Debug` carries a key or a record.
//!
//! Save order: the vault's anchor first, then the file. A crash in between leaves a file one
//! behind its anchor, which is refused and starts empty; it never leaves a file the anchor
//! would accept in place of a newer one.

use std::path::PathBuf;
use std::sync::Mutex;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use orbyn_device_client as dc;
use sha2::{Digest, Sha256};

use super::keystore::{Anchor, Vault};
use super::snapshot::SnapshotStore;

pub(crate) const PURPOSE: &str = "orbyn/engineering/snapshot";
pub(crate) const VERSION: u8 = 1;
const NONCE_LEN: usize = 24;
const HEADER_LEN: usize = 1 + 8 + NONCE_LEN;

/// Why a file was not used. For tests and a log line; it carries no content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Too short, or not a snapshot of ours.
    Unreadable,
    WrongVersion,
    /// The vault has no anchor, or the file's counter is lower than it: an older file.
    RolledBack,
    /// Same counter or a higher one, but not the file the vault last wrote.
    NotTheLatest,
    /// The seal does not open: another profile, another vault key or a changed file.
    DoesNotOpen,
}

pub(crate) struct SealedSnapshots {
    path: PathBuf,
    profile: String,
    vault: std::sync::Arc<dyn Vault>,
    refused: Mutex<Option<Refusal>>,
}

impl std::fmt::Debug for SealedSnapshots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SealedSnapshots(..)")
    }
}

fn bound(profile: &str, counter: u64) -> Vec<u8> {
    let mut out = Vec::new();
    for part in [PURPOSE.as_bytes(), profile.as_bytes()] {
        out.extend_from_slice(&(part.len() as u64).to_be_bytes());
        out.extend_from_slice(part);
    }
    out.push(VERSION);
    out.extend_from_slice(&counter.to_be_bytes());
    out
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

impl SealedSnapshots {
    pub(crate) fn new(
        path: impl Into<PathBuf>,
        profile: impl Into<String>,
        vault: std::sync::Arc<dyn Vault>,
    ) -> Self {
        Self {
            path: path.into(),
            profile: profile.into(),
            vault,
            refused: Mutex::new(None),
        }
    }

    /// Why the last load was refused, if it was.
    #[cfg(test)]
    pub(crate) fn last_refusal(&self) -> Option<Refusal> {
        *self
            .refused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn note(&self, refusal: Option<Refusal>) {
        if let Some(why) = refusal {
            tracing::warn!(?why, "the sealed account snapshot was not used");
        }
        *self
            .refused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = refusal;
    }

    fn open(&self, bytes: &[u8]) -> Result<dc::Snapshot, Refusal> {
        if bytes.len() <= HEADER_LEN {
            return Err(Refusal::Unreadable);
        }
        if bytes[0] != VERSION {
            return Err(Refusal::WrongVersion);
        }
        let counter = u64::from_be_bytes(bytes[1..9].try_into().map_err(|_| Refusal::Unreadable)?);
        let anchor = self.vault.anchor().ok_or(Refusal::RolledBack)?;
        if counter < anchor.counter {
            return Err(Refusal::RolledBack);
        }
        if counter != anchor.counter || digest(bytes) != anchor.hash {
            return Err(Refusal::NotTheLatest);
        }
        let key = self.vault.vault_key().ok_or(Refusal::DoesNotOpen)?;
        let nonce: [u8; NONCE_LEN] = bytes[9..HEADER_LEN]
            .try_into()
            .map_err(|_| Refusal::Unreadable)?;
        let plain = XChaCha20Poly1305::new(&(*key).into())
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &bytes[HEADER_LEN..],
                    aad: &bound(&self.profile, counter),
                },
            )
            .map(dc::Zeroizing::new)
            .map_err(|_| Refusal::DoesNotOpen)?;
        serde_json::from_slice(&plain).map_err(|_| Refusal::Unreadable)
    }

    fn seal(&self, snapshot: &dc::Snapshot) -> Option<(Vec<u8>, Anchor)> {
        let key = self.vault.vault_key()?;
        let counter = self
            .vault
            .anchor()
            .map_or(0, |anchor| anchor.counter)
            .saturating_add(1);
        let plain = dc::Zeroizing::new(serde_json::to_vec(snapshot).ok()?);
        let nonce = crate::util::random_bytes::<NONCE_LEN>().ok()?;
        let sealed = XChaCha20Poly1305::new(&(*key).into())
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &plain,
                    aad: &bound(&self.profile, counter),
                },
            )
            .ok()?;
        let mut out = Vec::with_capacity(HEADER_LEN + sealed.len());
        out.push(VERSION);
        out.extend_from_slice(&counter.to_be_bytes());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        let hash = digest(&out);
        Some((out, Anchor { counter, hash }))
    }
}

impl SnapshotStore for SealedSnapshots {
    fn load(&self) -> Option<dc::Snapshot> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.note(None);
                return None;
            }
        };
        match self.open(&bytes) {
            Ok(snapshot) => {
                self.note(None);
                Some(snapshot)
            }
            Err(why) => {
                self.note(Some(why));
                None
            }
        }
    }

    /// With no vault key (signed out, or wiped) nothing is written: a stale engine cannot put a
    /// snapshot back after a wipe.
    fn save(&self, snapshot: &dc::Snapshot) {
        let Some((bytes, anchor)) = self.seal(snapshot) else {
            return;
        };
        if self.vault.set_anchor(anchor).is_err() {
            tracing::warn!("the account snapshot was not saved: the anchor was not written");
            return;
        }
        if crate::util::write_private_atomic(&self.path, &bytes).is_err() {
            tracing::warn!("the account snapshot was not saved: the file was not written");
        }
    }

    fn clear(&self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => tracing::warn!("the sealed account snapshot could not be deleted"),
        }
    }
}
