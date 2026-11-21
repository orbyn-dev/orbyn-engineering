//! Where the keys are kept: the OS keychain only. The trait exists so tests use memory; there is no
//! file or database implementation, on purpose.

use std::sync::Arc;

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
