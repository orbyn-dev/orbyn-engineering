//! A file-backed `KeyStore` for development builds against a loopback Engine, and the vault it
//! gives the sealed snapshot. It exists in debug builds and tests only (the `mod` line carries
//! the cfg), like the `LocalOnly` gate it serves: a release has no file key store, because the
//! real one is the OS keychain and the account never falls back to a plain file
//! (docs/account.md, "The device and its keys" and the sign-in gate).
//!
//! The file holds the device identity (keys, ids, session token, vault key) and the counter and
//! hash of the latest sealed snapshot. It is as secret as the keys, so it is written owner-only
//! and replaced atomically. Nothing here is ever formatted: `Debug` prints no field, an error
//! carries no value, and a log line names no key.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use orbyn_device_client as dc;
use serde::{Deserialize, Serialize};

/// The counter and the hash of the newest sealed snapshot. They sit with the vault key, so
/// whoever can put an older snapshot file back cannot also move them back.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Anchor {
    pub counter: u64,
    pub hash: [u8; 32],
}

impl std::fmt::Debug for Anchor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Anchor(..)")
    }
}

/// What the sealed snapshot needs from the key store: the vault key and the anchor.
pub(crate) trait Vault: Send + Sync {
    fn vault_key(&self) -> Option<dc::Zeroizing<[u8; 32]>>;
    fn anchor(&self) -> Option<Anchor>;
    /// Fails when there is no identity to keep it with, or the file cannot be written.
    fn set_anchor(&self, anchor: Anchor) -> Result<(), dc::StoreError>;
}

#[derive(Serialize, Deserialize)]
struct DiskSession {
    token: String,
    expires_at: String,
}

#[derive(Serialize, Deserialize)]
struct DiskIdentity {
    device_keys: String,
    account_id: String,
    device_id: String,
    email: String,
    session: Option<DiskSession>,
    vault_key: String,
}

#[derive(Serialize, Deserialize)]
struct DiskAnchor {
    counter: u64,
    hash: String,
}

#[derive(Serialize, Deserialize)]
struct Disk {
    identity: DiskIdentity,
    anchor: Option<DiskAnchor>,
}

/// The development key store: one file, replaced atomically.
#[derive(Clone)]
pub struct FileKeyStore {
    path: Arc<PathBuf>,
    /// One writer at a time, so a load-modify-save never loses the anchor.
    lock: Arc<Mutex<()>>,
    /// A test can make deleting fail: 1 stops it after the overwrite, 2 stops it before anything,
    /// 3 lets the marker go down and then stops.
    #[cfg(test)]
    fail_deletes: Arc<std::sync::atomic::AtomicU8>,
}

impl std::fmt::Debug for FileKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileKeyStore(..)")
    }
}

impl FileKeyStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Arc::new(path.into()),
            lock: Arc::default(),
            #[cfg(test)]
            fail_deletes: Arc::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn fail_deletes(&self, mode: u8) {
        self.fail_deletes
            .store(mode, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(test)]
    fn failing(&self) -> u8 {
        self.fail_deletes.load(std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(not(test))]
    fn failing(&self) -> u8 {
        0
    }

    /// The "wipe owed" marker: it is written before a delete starts and removed only when the
    /// key file is gone. While it exists nothing reads the key file, even a half-deleted one.
    fn marker(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_owned();
        name.push(".wipe");
        PathBuf::from(name)
    }

    /// Overwrites the file with nothing, then removes it, then the marker. Each step runs even
    /// if an earlier one failed, and the first failure is reported.
    fn wipe_files(&self) -> Result<(), dc::StoreError> {
        if self.failing() >= 2 {
            return Err(dc::StoreError);
        }
        let mut result = Ok(());
        match std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&*self.path)
        {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => result = Err(dc::StoreError),
        }
        let removed = if self.failing() == 1 {
            Err(dc::StoreError)
        } else {
            match std::fs::remove_file(&*self.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(dc::StoreError),
            }
        };
        result = result.and(removed);
        // The marker goes only once the key file is gone.
        if result.is_ok() {
            match std::fs::remove_file(self.marker()) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => result = Err(dc::StoreError),
            }
        }
        result
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn read(&self) -> Result<Option<Disk>, dc::StoreError> {
        if self.marker().exists() {
            // A delete started and did not finish: the file is not to be used.
            return Err(dc::StoreError);
        }
        let bytes = match std::fs::read(&*self.path) {
            Ok(bytes) => dc::Zeroizing::new(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(dc::StoreError),
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| dc::StoreError)
    }

    fn write(&self, disk: &Disk) -> Result<(), dc::StoreError> {
        let bytes = dc::Zeroizing::new(serde_json::to_vec(disk).map_err(|_| dc::StoreError)?);
        crate::util::write_private_atomic(&self.path, &bytes).map_err(|_| dc::StoreError)
    }
}

fn decode<const N: usize>(text: &str) -> Result<dc::Zeroizing<[u8; N]>, dc::StoreError> {
    let bytes = dc::Zeroizing::new(B64.decode(text).map_err(|_| dc::StoreError)?);
    let mut out = dc::Zeroizing::new([0u8; N]);
    if bytes.len() != N {
        return Err(dc::StoreError);
    }
    out.copy_from_slice(&bytes);
    Ok(out)
}

impl dc::KeyStore for FileKeyStore {
    fn load(&self) -> Result<Option<dc::StoredIdentity>, dc::StoreError> {
        let _held = self.guard();
        let Some(disk) = self.read()? else {
            return Ok(None);
        };
        let id = disk.identity;
        Ok(Some(dc::StoredIdentity {
            device_keys: decode::<64>(&id.device_keys)?,
            account_id: id.account_id,
            device_id: id.device_id,
            email: id.email,
            session: id.session.map(|session| dc::StoredSession {
                token: dc::Zeroizing::new(session.token),
                expires_at: session.expires_at,
            }),
            vault_key: decode::<32>(&id.vault_key)?,
        }))
    }

    fn save(&self, identity: &dc::StoredIdentity) -> Result<(), dc::StoreError> {
        let _held = self.guard();
        if self.marker().exists() {
            // A new identity may only land once the old one is really gone.
            self.wipe_files()?;
        }
        // The anchor belongs to the vault key: it stays while the key does.
        let anchor = self.read()?.and_then(|disk| disk.anchor);
        let mut disk = Disk {
            identity: DiskIdentity {
                device_keys: B64.encode(identity.device_keys.as_slice()),
                account_id: identity.account_id.clone(),
                device_id: identity.device_id.clone(),
                email: identity.email.clone(),
                session: identity.session.as_ref().map(|session| DiskSession {
                    token: session.token.to_string(),
                    expires_at: session.expires_at.clone(),
                }),
                vault_key: B64.encode(identity.vault_key.as_slice()),
            },
            anchor,
        };
        let written = self.write(&disk);
        // The encoded secrets do not outlive the call.
        zeroize::Zeroize::zeroize(&mut disk.identity.device_keys);
        zeroize::Zeroize::zeroize(&mut disk.identity.vault_key);
        if let Some(session) = disk.identity.session.as_mut() {
            zeroize::Zeroize::zeroize(&mut session.token);
        }
        written
    }

    /// Deletes the file: the keys, the token, the vault key and the anchor go together. The
    /// marker goes down first and the file is overwritten with nothing before it is removed,
    /// so a delete that fails half way leaves no readable key. A failure is returned, and the
    /// next `clear` (or `save`) finishes it.
    fn clear(&self) -> Result<(), dc::StoreError> {
        let _held = self.guard();
        if self.failing() == 2 {
            return Err(dc::StoreError);
        }
        let marked = std::fs::write(self.marker(), b"wipe owed");
        let wiped = self.wipe_files();
        // No marker means a failed delete is not remembered here; the caller still hears of it.
        drop(marked);
        wiped
    }
}

impl Vault for FileKeyStore {
    fn vault_key(&self) -> Option<dc::Zeroizing<[u8; 32]>> {
        let _held = self.guard();
        let disk = self.read().ok()??;
        decode::<32>(&disk.identity.vault_key).ok()
    }

    fn anchor(&self) -> Option<Anchor> {
        let _held = self.guard();
        let anchor = self.read().ok()??.anchor?;
        let hash = decode::<32>(&anchor.hash).ok()?;
        Some(Anchor {
            counter: anchor.counter,
            hash: *hash,
        })
    }

    fn set_anchor(&self, anchor: Anchor) -> Result<(), dc::StoreError> {
        let _held = self.guard();
        let mut disk = self.read()?.ok_or(dc::StoreError)?;
        disk.anchor = Some(DiskAnchor {
            counter: anchor.counter,
            hash: B64.encode(anchor.hash),
        });
        self.write(&disk)
    }
}
