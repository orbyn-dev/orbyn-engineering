//! The Engine's errors as the app's, for the traits the screens and the worker use.

use std::time::Duration;

use orbyn_device_client as dc;

use crate::account::device_api::DeviceError;

/// The wait when the Engine rate-limits without saying for how long.
const DEFAULT_RETRY: Duration = Duration::from_secs(60);

/// None of these carries a request, a header, a key or a digit.
pub(super) fn map(error: dc::DeviceError) -> DeviceError {
    match error {
        dc::DeviceError::Transport => DeviceError::Offline,
        dc::DeviceError::ReauthenticationRequired => DeviceError::ReauthRequired,
        dc::DeviceError::InvalidCode | dc::DeviceError::SecondFactorFailed => {
            DeviceError::InvalidCode
        }
        dc::DeviceError::RateLimited { retry_after } => DeviceError::RateLimited {
            retry_after: retry_after.unwrap_or(DEFAULT_RETRY),
        },
        dc::DeviceError::DeletionScheduled => DeviceError::DeletionScheduled,
        dc::DeviceError::AccountGone => DeviceError::AccountGone,
        dc::DeviceError::TooLarge => DeviceError::TooLarge,
        dc::DeviceError::LoginFailed => DeviceError::WrongPassword,
        dc::DeviceError::InviteInvalid => DeviceError::InviteRequired,
        dc::DeviceError::NotSignedIn | dc::DeviceError::SessionExpired => DeviceError::Unauthorized,
        dc::DeviceError::StoreNotEmpty => DeviceError::AlreadySignedIn,
        dc::DeviceError::ApprovalExpired => DeviceError::RequestGone,
        dc::DeviceError::ApprovalMismatch => DeviceError::ApprovalRefused,
        dc::DeviceError::LastTrustedDevice => DeviceError::LastTrustedDevice,
        dc::DeviceError::ApprovalNotReady | dc::DeviceError::NoZoneKey => {
            DeviceError::Server { retryable: true }
        }
        dc::DeviceError::Store(_) => DeviceError::Server { retryable: true },
        // A server that answers 409 or 410 is asking for another pass, not refusing.
        dc::DeviceError::Server { status, .. } => DeviceError::Server {
            retryable: status >= 500 || status == 409 || status == 410,
        },
        dc::DeviceError::DeviceNotTrusted
        | dc::DeviceError::InvalidInput(_)
        | dc::DeviceError::ZoneDeleted
        | dc::DeviceError::Engine(_)
        | dc::DeviceError::Crypto => DeviceError::Server { retryable: false },
    }
}

#[cfg(test)]
pub(crate) fn map_for_test(error: dc::DeviceError) -> DeviceError {
    map(error)
}
