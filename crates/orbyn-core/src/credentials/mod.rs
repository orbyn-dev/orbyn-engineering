//! Per-engine API keys in the OS keychain, by the person's opt-in. Until now Orbyn never read, stored
//! or forwarded an engine's token (docs/architecture.md). This module is the one exception, and it is
//! narrow:
//!
//! - **Opt-in per engine.** Nothing is read or written unless the caller says the person turned the
//!   setting on for that engine (`enabled`), and only for the engines in [`ENGINES`].
//! - **Keychain only.** One generic-password item per engine in the OS keychain. There is no file, no
//!   database row and no fallback store: with no usable keychain, [`Status::Unavailable`] and nothing
//!   is saved.
//! - **Never shown again.** The key is write-only to the person. The only way out is
//!   [`Credentials::take_for_spawn`], which a driver calls to put the key in a child's environment
//!   (for example `CURSOR_API_KEY`), never in a frame, a log, an error or a route's answer.
//! - **Revocable.** [`Credentials::remove`] deletes the item.
//!
//! No value is ever formatted: [`SpawnSecret`] prints nothing in `Debug`, zeroizes on drop, and the
//! errors carry no key text. The web page and the routes get [`Status`] only.

mod backend;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use zeroize::Zeroizing;

#[cfg(all(target_os = "macos", feature = "engine-client"))]
pub use backend::SystemKeychain;
pub use backend::{Failure, SecretBackend};

/// The keychain service name of a development or unsigned build; a signed release uses its own, so
/// neither reads the other's items (the same rule as the account device, docs/account.md).
pub const DEV_SERVICE: &str = "app.orbyn.engineering.engine-keys.dev";

/// The longest key we accept. Real keys are far shorter; the bound keeps a paste of a file out.
pub const MAX_KEY_LEN: usize = 512;

/// An engine that may keep a key, and the environment variable its host reads at spawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineKey {
    pub engine: &'static str,
    pub env: &'static str,
}

/// The allowlist. Adding an engine is a code change and a security-review item, never a setting.
pub const ENGINES: &[EngineKey] = &[EngineKey {
    engine: "cursor",
    env: "CURSOR_API_KEY",
}];

/// What the person may be told. Never the key, not even its length or last characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The opt-in is off for this engine.
    Off,
    /// No usable keychain on this platform or session; nothing can be saved.
    Unavailable,
    /// The keychain exists and refused for now (locked, denied, prompt cancelled).
    Blocked,
    /// Opted in and no key is saved.
    NotSaved,
    /// A key is saved in the keychain.
    Saved,
}

/// Why a call failed. No variant carries key text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    #[error("this engine does not keep a key")]
    UnknownEngine,
    #[error("keeping a key for this engine is off")]
    Off,
    #[error("the key is empty, too long or has a space or line break")]
    InvalidKey,
    #[error("there is no usable keychain here")]
    Unavailable,
    #[error("the keychain refused for now")]
    Blocked,
    #[error("the keychain failed")]
    Failed,
}

/// A key on its way into a child's environment. It prints nothing and zeroizes when dropped.
pub struct SpawnSecret {
    pub env: &'static str,
    value: Zeroizing<String>,
}

impl SpawnSecret {
    /// The value, for `Command::env` only.
    pub fn expose(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Debug for SpawnSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnSecret")
            .field("env", &self.env)
            .finish_non_exhaustive()
    }
}

/// The keychain-backed store. `service` is [`DEV_SERVICE`] in an unsigned build.
#[derive(Clone)]
pub struct Credentials {
    backend: Arc<dyn SecretBackend>,
    service: String,
}

impl Credentials {
    pub fn new(backend: Arc<dyn SecretBackend>, service: impl Into<String>) -> Self {
        Self {
            backend,
            service: service.into(),
        }
    }

    /// The macOS login keychain; on other platforms there is no backend, so every call says unavailable.
    pub fn system(service: impl Into<String>) -> Self {
        Self::new(backend::system(), service)
    }

    fn entry(engine: &str) -> Result<&'static EngineKey, CredentialError> {
        ENGINES
            .iter()
            .find(|e| e.engine == engine)
            .ok_or(CredentialError::UnknownEngine)
    }

    fn map(failure: Failure) -> CredentialError {
        match failure {
            Failure::Unusable => CredentialError::Unavailable,
            Failure::Blocked => CredentialError::Blocked,
            Failure::Other => CredentialError::Failed,
        }
    }

    /// The state the person sees. `enabled` is the engine's opt-in setting.
    pub fn status(&self, engine: &str, enabled: bool) -> Result<Status, CredentialError> {
        let entry = Self::entry(engine)?;
        if !enabled {
            return Ok(Status::Off);
        }
        match self.backend.get(&self.service, entry.engine) {
            Ok(Some(bytes)) if !bytes.is_empty() => Ok(Status::Saved),
            Ok(_) => Ok(Status::NotSaved),
            Err(Failure::Unusable) => Ok(Status::Unavailable),
            Err(Failure::Blocked) => Ok(Status::Blocked),
            Err(Failure::Other) => Err(CredentialError::Failed),
        }
    }

    /// Saves or replaces the key. The caller drops its copy of `key` right after.
    pub fn save(&self, engine: &str, enabled: bool, key: &str) -> Result<(), CredentialError> {
        let entry = Self::entry(engine)?;
        if !enabled {
            return Err(CredentialError::Off);
        }
        let key = key.trim();
        if key.is_empty()
            || key.len() > MAX_KEY_LEN
            || key.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(CredentialError::InvalidKey);
        }
        self.backend
            .set(&self.service, entry.engine, key.as_bytes())
            .map_err(Self::map)
    }

    /// Revokes: deletes the item. Works with the opt-in off too, so turning it off can always clean up.
    pub fn remove(&self, engine: &str) -> Result<(), CredentialError> {
        let entry = Self::entry(engine)?;
        self.backend
            .delete(&self.service, entry.engine)
            .map_err(Self::map)
    }

    /// The key for a child's environment, or `None` when the opt-in is off or nothing is saved.
    /// This is the only way a key leaves the keychain.
    pub fn take_for_spawn(
        &self,
        engine: &str,
        enabled: bool,
    ) -> Result<Option<SpawnSecret>, CredentialError> {
        let entry = Self::entry(engine)?;
        if !enabled {
            return Ok(None);
        }
        match self
            .backend
            .get(&self.service, entry.engine)
            .map_err(Self::map)?
        {
            Some(bytes) if !bytes.is_empty() => {
                let value =
                    String::from_utf8(bytes.to_vec()).map_err(|_| CredentialError::Failed)?;
                Ok(Some(SpawnSecret {
                    env: entry.env,
                    value: Zeroizing::new(value),
                }))
            }
            _ => Ok(None),
        }
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}
