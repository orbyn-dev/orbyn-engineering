//! Which key store a development profile uses, decided once and then fixed. A profile's keys live in
//! the macOS keychain or in the development file store, never both and never switching quietly:
//! a run that could not reach the keychain and fell back to a file would look signed out, create
//! a second identity, and leave the first one in the keychain to come back on the next run.
//!
//! The choice is a small marker in the profile's data folder, written on first use. With no
//! marker: the file store is used only when there is no keychain to ask (so no item can exist)
//! or when a file store for this profile already exists from before; a keychain that is locked,
//! refused or failing is an error the caller retries, never a reason to switch. With a marker
//! the choice is final, and a keychain profile whose keychain does not answer is unavailable.
//!
//! A wipe clears both stores (see [`Paired`]): the one in use and, best effort, the other.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use orbyn_device_client as dc;

use super::keychain::{KeychainKeyStore, Probe};
use super::keystore::{Anchor, Vault};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Keychain,
    File,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Self::Keychain => "keychain",
            Self::File => "file",
        }
    }
}

/// The profile's keys cannot be reached right now, or its stores disagree. Retry; nothing was
/// switched and no identity was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Unavailable;

fn marker(dir: &Path, profile: &str) -> PathBuf {
    dir.join(format!("account-keys-{profile}.store"))
}

fn remember(dir: &Path, profile: &str, kind: Kind) -> Result<Kind, Unavailable> {
    crate::util::write_private_atomic(&marker(dir, profile), kind.word().as_bytes())
        .map_err(|_| Unavailable)?;
    Ok(kind)
}

/// Decides, and records, which store `profile` uses. `keychain` is `None` where there is no
/// keychain backend at all; `file_exists` says a file store for the profile is already on disk.
pub(crate) fn choose(
    dir: &Path,
    profile: &str,
    keychain: Option<&KeychainKeyStore>,
    file_exists: bool,
) -> Result<Kind, Unavailable> {
    match std::fs::read_to_string(marker(dir, profile)) {
        Ok(text) => {
            return match text.trim() {
                "file" => Ok(Kind::File),
                "keychain" => match keychain.map(KeychainKeyStore::probe) {
                    Some(Probe::Works) => Ok(Kind::Keychain),
                    // Final: no switching to the file store because the keychain is away.
                    _ => Err(Unavailable),
                },
                _ => Err(Unavailable),
            };
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(Unavailable),
    }
    let Some(keychain) = keychain else {
        return remember(dir, profile, Kind::File);
    };
    match keychain.probe() {
        Probe::Works => {
            // Both holding this profile's identity means two identities: refuse, never pick.
            if file_exists && keychain.holds_identity().map_err(|_| Unavailable)? {
                return Err(Unavailable);
            }
            remember(
                dir,
                profile,
                if file_exists {
                    Kind::File
                } else {
                    Kind::Keychain
                },
            )
        }
        // No keychain to ask, so none holds an identity: the file store is the only place.
        Probe::Unusable => remember(dir, profile, Kind::File),
        // A keychain that did not answer may hold the identity: do not choose.
        Probe::Blocked => Err(Unavailable),
    }
}

/// A key store whose wipe also clears the profile's other store, best effort, so a wipe of the one
/// in use never leaves the other's identity to come back. The failure of the one in use is returned.
#[derive(Clone)]
pub(crate) struct Paired<K> {
    pub(crate) primary: K,
    pub(crate) other: Arc<dyn Fn() + Send + Sync>,
}

impl<K> std::fmt::Debug for Paired<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Paired(..)")
    }
}

impl<K: dc::KeyStore> dc::KeyStore for Paired<K> {
    fn load(&self) -> Result<Option<dc::StoredIdentity>, dc::StoreError> {
        self.primary.load()
    }

    fn save(&self, identity: &dc::StoredIdentity) -> Result<(), dc::StoreError> {
        self.primary.save(identity)
    }

    fn clear(&self) -> Result<(), dc::StoreError> {
        let cleared = self.primary.clear();
        (self.other)();
        cleared
    }
}

impl<K: Vault> Vault for Paired<K> {
    fn vault_key(&self) -> Option<dc::Zeroizing<[u8; 32]>> {
        self.primary.vault_key()
    }

    fn anchor(&self) -> Option<Anchor> {
        self.primary.anchor()
    }

    fn set_anchor(&self, anchor: Anchor) -> Result<(), dc::StoreError> {
        self.primary.set_anchor(anchor)
    }
}
