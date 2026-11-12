//! Windows and Linux: no restriction is available, so the core refuses a crew worker.

use super::{Confinement, ResolvedCommand, UNSUPPORTED_PLATFORM};
use crate::error::{CoreError, Result};

pub fn unavailable() -> Option<String> {
    Some(UNSUPPORTED_PLATFORM.to_owned())
}

pub fn wrap(_: &Confinement, _: &mut ResolvedCommand) -> Result<()> {
    Err(CoreError::EngineUnavailable(
        UNSUPPORTED_PLATFORM.to_owned(),
    ))
}
