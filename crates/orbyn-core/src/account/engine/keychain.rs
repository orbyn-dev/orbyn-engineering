//! The OS keychain `KeyStore`: the device identity (keys, ids, session token, vault key) and the
//! sealed snapshot's anchor, kept as one keychain item per profile. The macOS backend is
//! `SystemKeychain`, over the `security-framework` crate (MIT or Apache-2.0; the desktop lock
//! already carries it through the platform certificate verifier). It uses the login keychain's
//! generic password items, which need no entitlement, so it works in an unsigned build.
//!
//! What signing adds is not here yet: the access list that trusts only `orbynd` by its code
//! signature needs a Developer ID (docs/account.md, sign-in gate item 5; threat model control 1).
//! Until then an item carries the keychain's default access, and the service name says it is a
//! development item ([`DEV_SERVICE`]) so a release never trusts or reads one.
//!
//! A delete is never silent. `clear` removes the item; when that fails it overwrites the item
//! with nothing (a tombstone, which no read treats as an identity) and returns the failure, so a
//! key whose delete failed is not a usable key. The next `load`, `save` or `clear` retries the
//! delete, also after a restart: the item itself says a wipe is owed. Limit: if the keychain
//! refuses both the delete and the overwrite, nothing can be recorded; `Shared` keeps the wipe
//! owed in memory for the run and the failure is logged.
//!
//! No value is formatted: `Debug` prints no field, errors carry no value, and the encoded record
//! is zeroized after it is written.

use std::sync::{Arc, Mutex};

use orbyn_device_client as dc;

use super::keystore::{
    Anchor, Vault, anchor_of, disk_anchor, disk_of, encode_disk, identity_of, parse_disk, wipe_disk,
};

/// The service name a development or unsigned build uses. A signed release uses a different
/// one, so neither can read the other's items.
pub const DEV_SERVICE: &str = "app.orbyn.engineering.account.dev";

/// Why the keychain did not answer. The difference decides what the app may do: with no keychain at
/// all there can be no item, so a development build may use its file store; when the keychain is
/// locked, refused or failing it may hold this profile's identity, so nothing may switch stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// There is no keychain to ask (no default keychain, not available in this session).
    Unusable,
    /// There is one and it said no for now: locked, interaction not allowed, a refused prompt.
    Blocked,
    /// Anything else, which may pass.
    Other,
}

/// Where the secrets are kept. `get` answers `None` for no item; `delete` of no item is fine.
pub(crate) trait SecretBackend: Send + Sync {
    fn get(&self, service: &str, account: &str) -> Result<Option<dc::Zeroizing<Vec<u8>>>, Failure>;
    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), Failure>;
    fn delete(&self, service: &str, account: &str) -> Result<(), Failure>;
}

/// What the probe found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Probe {
    Works,
    /// No keychain exists here: no item can exist either.
    Unusable,
    /// A keychain exists and did not answer now. It may hold this profile's identity.
    Blocked,
}

/// The macOS login keychain.
#[cfg(target_os = "macos")]
pub struct SystemKeychain;

#[cfg(target_os = "macos")]
impl SecretBackend for SystemKeychain {
    fn get(&self, service: &str, account: &str) -> Result<Option<dc::Zeroizing<Vec<u8>>>, Failure> {
        use security_framework::passwords::{PasswordOptions, generic_password};
        match generic_password(PasswordOptions::new_generic_password(service, account)) {
            Ok(bytes) => Ok(Some(dc::Zeroizing::new(bytes))),
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(error) => Err(failure_of(error.code())),
        }
    }

    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), Failure> {
        security_framework::passwords::set_generic_password(service, account, bytes)
            .map_err(|error| failure_of(error.code()))
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), Failure> {
        match security_framework::passwords::delete_generic_password(service, account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(error) => Err(failure_of(error.code())),
        }
    }
}

/// `errSecItemNotFound`.
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// The Security framework's status, as the decision it drives: no keychain at all
/// (`errSecNotAvailable`, `errSecNoSuchKeychain`, `errSecNoDefaultKeychain`), a keychain that
/// said no for now (`errSecInteractionNotAllowed`, `errSecAuthFailed`, `errSecUserCanceled`,
/// `errSecInvalidKeychain`'s locked cousin `errSecReadOnly`), or something else.
#[cfg(target_os = "macos")]
fn failure_of(code: i32) -> Failure {
    match code {
        -25291 | -25294 | -25307 => Failure::Unusable,
        -25308 | -25293 | -128 | -25295 | -61 => Failure::Blocked,
        _ => Failure::Other,
    }
}

/// The keychain-backed store for one profile.
#[derive(Clone)]
pub struct KeychainKeyStore {
    backend: Arc<dyn SecretBackend>,
    service: String,
    account: String,
    /// One writer at a time, so a load-modify-save never loses the anchor.
    lock: Arc<Mutex<()>>,
}

impl std::fmt::Debug for KeychainKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeychainKeyStore(..)")
    }
}

/// What the item holds.
enum Item {
    Absent,
    /// A delete failed and the item was blanked: a wipe is owed.
    Tombstone,
    Record(Box<super::keystore::Disk>),
}

impl KeychainKeyStore {
    pub(crate) fn new(backend: Arc<dyn SecretBackend>, service: &str, profile: &str) -> Self {
        Self {
            backend,
            service: service.to_owned(),
            // The account name comes from the profile id: two data folders never share keys.
            account: format!("profile-{profile}"),
            lock: Arc::default(),
        }
    }

    /// The macOS login keychain, for a development or unsigned build.
    #[cfg(target_os = "macos")]
    pub fn system(profile: &str) -> Self {
        Self::new(Arc::new(SystemKeychain), DEV_SERVICE, profile)
    }

    /// Whether the keychain answers: one throwaway item is written, read back and deleted. Its
    /// name is fixed per profile, so a probe that could not delete itself is overwritten and
    /// deleted by the next one. A missing keychain and a keychain that said no are different
    /// answers: only the first may let a development build use its file store.
    pub(crate) fn probe(&self) -> Probe {
        let probe = format!("{}-probe", self.account);
        let answer = self
            .backend
            .set(&self.service, &probe, b"probe")
            .and_then(|()| self.backend.get(&self.service, &probe));
        let _ = self.backend.delete(&self.service, &probe);
        match answer {
            Ok(Some(read)) if read.as_slice() == b"probe" => Probe::Works,
            Err(Failure::Unusable) => Probe::Unusable,
            // A locked keychain, a refused prompt, a read that did not match, a transient error.
            _ => Probe::Blocked,
        }
    }

    /// Whether an item for this profile exists. `Err` when the keychain cannot say.
    pub(crate) fn holds_identity(&self) -> Result<bool, Failure> {
        Ok(self.backend.get(&self.service, &self.account)?.is_some())
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn read(&self) -> Result<Item, dc::StoreError> {
        match self
            .backend
            .get(&self.service, &self.account)
            .map_err(|_| dc::StoreError)?
        {
            None => Ok(Item::Absent),
            Some(bytes) if bytes.is_empty() => Ok(Item::Tombstone),
            Some(bytes) => parse_disk(&bytes).map(|disk| Item::Record(Box::new(disk))),
        }
    }

    /// Finishes a wipe that was owed: the item goes, or the failure is returned.
    fn finish_wipe(&self) -> Result<(), dc::StoreError> {
        self.backend
            .delete(&self.service, &self.account)
            .map_err(|_| dc::StoreError)
    }

    fn write(&self, disk: &mut super::keystore::Disk) -> Result<(), dc::StoreError> {
        let bytes = encode_disk(disk)?;
        let written = self
            .backend
            .set(&self.service, &self.account, &bytes)
            .map_err(|_| dc::StoreError);
        wipe_disk(disk);
        written
    }
}

impl dc::KeyStore for KeychainKeyStore {
    fn load(&self) -> Result<Option<dc::StoredIdentity>, dc::StoreError> {
        let _held = self.guard();
        match self.read()? {
            Item::Absent => Ok(None),
            Item::Tombstone => {
                // A wipe was owed (this run or the last): every start of the Engine and every
                // sign-in loads first, so it is finished here or its failure is returned.
                self.finish_wipe()?;
                Ok(None)
            }
            Item::Record(mut disk) => {
                let identity = identity_of(std::mem::take(&mut disk.identity));
                wipe_disk(&mut disk);
                identity.map(Some)
            }
        }
    }

    fn save(&self, identity: &dc::StoredIdentity) -> Result<(), dc::StoreError> {
        let _held = self.guard();
        let anchor = match self.read()? {
            Item::Tombstone => {
                // A new identity may only land once the old one is really gone.
                self.finish_wipe()?;
                None
            }
            Item::Absent => None,
            Item::Record(disk) => disk.anchor,
        };
        let mut disk = disk_of(identity, anchor);
        self.write(&mut disk)
    }

    /// Removes the item. When that fails the item is blanked (so it is no usable key) and the
    /// failure is returned; the next `load`, `save` or `clear` finishes the delete.
    fn clear(&self) -> Result<(), dc::StoreError> {
        let _held = self.guard();
        match self.finish_wipe() {
            Ok(()) => Ok(()),
            Err(error) => {
                let blanked = self.backend.set(&self.service, &self.account, b"");
                tracing::warn!(
                    blanked = blanked.is_ok(),
                    "the keychain item could not be deleted; the wipe is owed"
                );
                Err(error)
            }
        }
    }
}

impl Vault for KeychainKeyStore {
    fn vault_key(&self) -> Option<dc::Zeroizing<[u8; 32]>> {
        let _held = self.guard();
        match self.read().ok()? {
            Item::Record(mut disk) => {
                let key = identity_of(std::mem::take(&mut disk.identity)).ok();
                wipe_disk(&mut disk);
                key.map(|id| id.vault_key)
            }
            _ => None,
        }
    }

    fn anchor(&self) -> Option<Anchor> {
        let _held = self.guard();
        match self.read().ok()? {
            Item::Record(mut disk) => {
                let anchor = disk.anchor.as_ref().and_then(anchor_of);
                wipe_disk(&mut disk);
                anchor
            }
            _ => None,
        }
    }

    fn set_anchor(&self, anchor: Anchor) -> Result<(), dc::StoreError> {
        let _held = self.guard();
        let Item::Record(mut disk) = self.read()? else {
            return Err(dc::StoreError);
        };
        disk.anchor = Some(disk_anchor(anchor));
        self.write(&mut disk)
    }
}
