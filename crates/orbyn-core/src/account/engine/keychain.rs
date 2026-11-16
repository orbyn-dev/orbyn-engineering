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

/// Where the secrets are kept. `get` answers `None` for no item; `delete` of no item is fine.
pub(crate) trait SecretBackend: Send + Sync {
    fn get(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<dc::Zeroizing<Vec<u8>>>, dc::StoreError>;
    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), dc::StoreError>;
    fn delete(&self, service: &str, account: &str) -> Result<(), dc::StoreError>;
}

/// The macOS login keychain.
#[cfg(target_os = "macos")]
pub struct SystemKeychain;

#[cfg(target_os = "macos")]
impl SecretBackend for SystemKeychain {
    fn get(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<dc::Zeroizing<Vec<u8>>>, dc::StoreError> {
        use security_framework::passwords::{PasswordOptions, generic_password};
        match generic_password(PasswordOptions::new_generic_password(service, account)) {
            Ok(bytes) => Ok(Some(dc::Zeroizing::new(bytes))),
            Err(error) if error.code() == security_framework_sys_not_found() => Ok(None),
            Err(_) => Err(dc::StoreError),
        }
    }

    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), dc::StoreError> {
        security_framework::passwords::set_generic_password(service, account, bytes)
            .map_err(|_| dc::StoreError)
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), dc::StoreError> {
        match security_framework::passwords::delete_generic_password(service, account) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == security_framework_sys_not_found() => Ok(()),
            Err(_) => Err(dc::StoreError),
        }
    }
}

/// `errSecItemNotFound`.
#[cfg(target_os = "macos")]
const fn security_framework_sys_not_found() -> i32 {
    -25300
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

    /// Whether the keychain answers: a throwaway item is written, read back and deleted. A
    /// locked or missing keychain, or a user who refuses the prompt, is `false`.
    pub fn available(&self) -> bool {
        let probe = format!("{}-probe-{}", self.account, crate::util::new_id());
        let ok = self
            .backend
            .set(&self.service, &probe, b"probe")
            .and_then(|()| self.backend.get(&self.service, &probe))
            .is_ok_and(|read| read.is_some_and(|read| read.as_slice() == b"probe"));
        let _ = self.backend.delete(&self.service, &probe);
        ok
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn read(&self) -> Result<Item, dc::StoreError> {
        match self.backend.get(&self.service, &self.account)? {
            None => Ok(Item::Absent),
            Some(bytes) if bytes.is_empty() => Ok(Item::Tombstone),
            Some(bytes) => parse_disk(&bytes).map(|disk| Item::Record(Box::new(disk))),
        }
    }

    /// Finishes a wipe that was owed: the item goes, or the failure is returned.
    fn finish_wipe(&self) -> Result<(), dc::StoreError> {
        self.backend.delete(&self.service, &self.account)
    }

    fn write(&self, disk: &mut super::keystore::Disk) -> Result<(), dc::StoreError> {
        let bytes = encode_disk(disk)?;
        let written = self.backend.set(&self.service, &self.account, &bytes);
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
            Item::Record(disk) => identity_of(disk.identity).map(Some),
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
            Item::Record(disk) => identity_of(disk.identity).ok().map(|id| id.vault_key),
            _ => None,
        }
    }

    fn anchor(&self) -> Option<Anchor> {
        let _held = self.guard();
        match self.read().ok()? {
            Item::Record(disk) => anchor_of(&disk.anchor?),
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
