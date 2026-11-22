//! Where the keys are kept: the OS keychain only. The trait exists so tests use memory; there is no
//! file or database implementation, on purpose.

use std::sync::Arc;
#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::{AtomicUsize, Ordering};

use zeroize::Zeroizing;

/// Why the keychain did not answer, as the decision it drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// No keychain to ask: nothing can be saved here.
    Unusable,
    /// There is one and it said no for now: locked, interaction not allowed, a refused prompt.
    Blocked,
    /// Anything else, which may pass.
    Other,
}

/// `get` answers `None` for no item; `delete` of no item is fine.
pub trait SecretBackend: Send + Sync {
    fn get(&self, service: &str, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Failure>;
    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), Failure>;
    fn delete(&self, service: &str, account: &str) -> Result<(), Failure>;
}

/// A platform with no keychain backend: every call says unusable, nothing is stored.
#[cfg(not(all(
    target_os = "macos",
    any(feature = "engine-client", feature = "engine-credentials")
)))]
struct NoKeychain;

#[cfg(not(all(
    target_os = "macos",
    any(feature = "engine-client", feature = "engine-credentials")
)))]
impl SecretBackend for NoKeychain {
    fn get(&self, _: &str, _: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Failure> {
        Err(Failure::Unusable)
    }
    fn set(&self, _: &str, _: &str, _: &[u8]) -> Result<(), Failure> {
        Err(Failure::Unusable)
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), Failure> {
        Err(Failure::Unusable)
    }
}

pub(super) fn system() -> Arc<dyn SecretBackend> {
    #[cfg(all(
        target_os = "macos",
        any(feature = "engine-client", feature = "engine-credentials")
    ))]
    {
        Arc::new(SystemKeychain)
    }
    #[cfg(not(all(
        target_os = "macos",
        any(feature = "engine-client", feature = "engine-credentials")
    )))]
    {
        Arc::new(NoKeychain)
    }
}

/// The macOS login keychain: generic password items, which need no entitlement.
#[cfg(all(
    target_os = "macos",
    any(feature = "engine-client", feature = "engine-credentials")
))]
pub struct SystemKeychain;

#[cfg(all(
    target_os = "macos",
    any(feature = "engine-client", feature = "engine-credentials")
))]
impl SecretBackend for SystemKeychain {
    fn get(&self, service: &str, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Failure> {
        use security_framework::passwords::{PasswordOptions, generic_password};
        match generic_password(PasswordOptions::new_generic_password(service, account)) {
            Ok(bytes) => Ok(Some(Zeroizing::new(bytes))),
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

#[cfg(all(
    target_os = "macos",
    any(feature = "engine-client", feature = "engine-credentials")
))]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// No keychain at all (-25291, -25294, -25307); a keychain that said no for now (-25308, -25293, -128,
/// -25295, -25292, -61); anything else may pass. Same mapping as the account device's keychain.
#[cfg(all(
    target_os = "macos",
    any(feature = "engine-client", feature = "engine-credentials")
))]
fn failure_of(code: i32) -> Failure {
    match code {
        -25291 | -25294 | -25307 => Failure::Unusable,
        -25308 | -25293 | -128 | -25295 | -25292 | -61 => Failure::Blocked,
        _ => Failure::Other,
    }
}

/// An in-memory keychain for tests, with a switch that makes every call fail the way a real keychain can. Never a product backend.
#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub struct MemoryBackend {
    items: std::sync::Mutex<std::collections::HashMap<(String, String), Vec<u8>>>,
    fail: std::sync::Mutex<Option<Failure>>,
    gets: AtomicUsize,
}

#[cfg(any(test, feature = "test-support"))]
impl MemoryBackend {
    /// Every later call fails with this, or works again with `None`.
    pub fn fail_with(&self, failure: Option<Failure>) {
        *self.fail.lock().unwrap() = failure;
    }

    /// Every `(service, account)` that holds an item.
    pub fn accounts(&self) -> Vec<(String, String)> {
        let mut all: Vec<_> = self.items.lock().unwrap().keys().cloned().collect();
        all.sort();
        all
    }

    /// How many `get` calls every use of this backend has made, failed ones included. The env-set state check is one.
    pub fn get_calls(&self) -> usize {
        self.gets.load(Ordering::Relaxed)
    }

    /// The item's bytes, for a test that proves a value reached the keychain.
    pub fn bytes(&self, service: &str, account: &str) -> Option<Vec<u8>> {
        self.items
            .lock()
            .unwrap()
            .get(&(service.into(), account.into()))
            .cloned()
    }

    fn check(&self) -> Result<(), Failure> {
        self.fail.lock().unwrap().map_or(Ok(()), Err)
    }
}

#[cfg(any(test, feature = "test-support"))]
impl SecretBackend for MemoryBackend {
    fn get(&self, service: &str, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Failure> {
        self.gets.fetch_add(1, Ordering::Relaxed);
        self.check()?;
        Ok(self.bytes(service, account).map(Zeroizing::new))
    }
    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), Failure> {
        self.check()?;
        self.items
            .lock()
            .unwrap()
            .insert((service.into(), account.into()), bytes.to_vec());
        Ok(())
    }
    fn delete(&self, service: &str, account: &str) -> Result<(), Failure> {
        self.check()?;
        self.items
            .lock()
            .unwrap()
            .remove(&(service.into(), account.into()));
        Ok(())
    }
}
