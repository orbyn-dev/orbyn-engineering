//! Environment-set values in the OS keychain (docs/execution-profiles-and-library.md, Part 2). A second, owner-approved keychain
//! use beside the engine keys, in a **separate service name** so no engine-key code path can reach a set and no set path can reach an
//! engine key. One generic-password item per key, account `<set id>/<KEY>`, the value's bytes as data.
//!
//! The same rules as the engine keys: keychain only (no file, no row, no fallback: with no usable keychain nothing is saved),
//! write-only to the person, and the only way out is [`EnvSets::spawn_values`], which hands [`EnvSecret`]s to the one caller that
//! starts a workflow's command. An [`EnvSecret`] prints nothing in `Debug` and zeroizes on drop; no error here carries a value.

use std::sync::Arc;

use orbyn_contracts::EnvKeychain;
use zeroize::Zeroizing;

use super::backend::{self, Failure, SecretBackend};

/// The keychain service of a development or unsigned build (the same split as [`super::DEV_SERVICE`]).
pub const ENV_DEV_SERVICE: &str = "app.orbyn.engineering.env-sets.dev";

/// The keychain service of a release build.
pub const ENV_RELEASE_SERVICE: &str = "app.orbyn.engineering.env-sets";

/// The service this build uses.
pub const fn default_env_service() -> &'static str {
    if cfg!(debug_assertions) {
        ENV_DEV_SERVICE
    } else {
        ENV_RELEASE_SERVICE
    }
}

/// A value is 1 to this many bytes.
pub const MAX_VALUE_BYTES: usize = 8192;
/// A key name is at most this long.
pub const MAX_KEY_NAME: usize = 128;

/// Why a key name is not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyProblem {
    /// Not `[A-Z_][A-Z0-9_]{0,127}`.
    Malformed,
    /// A name that changes what runs or leaks Orbyn's own state.
    Refused,
}

/// Names that change what a shell or an interpreter runs, or that are Orbyn's own: refused whatever the set.
const REFUSED: &[&str] = &[
    "PATH",
    "HOME",
    "SHELL",
    "IFS",
    "NODE_OPTIONS",
    "PYTHONPATH",
    "PYTHONSTARTUP",
    "BASH_ENV",
    "ENV",
    "SHELLOPTS",
    "BASHOPTS",
    "PROMPT_COMMAND",
    "PS4",
    "ZDOTDIR",
    "CURSOR_API_KEY",
];

/// Name prefixes refused for the same reason: the loader, Orbyn's own state and the engine switches the child environment sets.
const REFUSED_PREFIXES: &[&str] = &["LD_", "DYLD_", "ORBYN_", "CLAUDE", "CODEX"];

/// Checks a key name against the shape and the refused list. A name is not secret, and is never put in an error with a value.
pub fn check_key_name(name: &str) -> Result<(), KeyProblem> {
    let mut chars = name.chars();
    let shaped = name.len() <= MAX_KEY_NAME
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if !shaped {
        return Err(KeyProblem::Malformed);
    }
    if REFUSED.contains(&name)
        || REFUSED_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
    {
        return Err(KeyProblem::Refused);
    }
    Ok(())
}

/// Why a call failed. No variant carries a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EnvError {
    #[error("the value is empty, too long or has a NUL byte")]
    InvalidValue,
    #[error("there is no usable keychain here")]
    Unavailable,
    #[error("the keychain refused for now")]
    Blocked,
    #[error("the keychain failed")]
    Failed,
}

fn map(failure: Failure) -> EnvError {
    match failure {
        Failure::Unusable => EnvError::Unavailable,
        Failure::Blocked => EnvError::Blocked,
        Failure::Other => EnvError::Failed,
    }
}

/// A value on its way into a workflow run's environment. It prints nothing and zeroizes when dropped.
pub struct EnvSecret {
    pub key: String,
    value: Zeroizing<String>,
}

impl EnvSecret {
    /// The value, for the child's environment and the run's output mask only.
    pub fn expose(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Debug for EnvSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvSecret")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// The keychain-backed store of environment-set values.
#[derive(Clone)]
pub struct EnvSets {
    backend: Arc<dyn SecretBackend>,
    service: String,
}

impl EnvSets {
    pub fn new(backend: Arc<dyn SecretBackend>, service: impl Into<String>) -> Self {
        Self {
            backend,
            service: service.into(),
        }
    }

    /// The macOS login keychain; elsewhere there is no backend, so every call says unavailable.
    pub fn system(service: impl Into<String>) -> Self {
        Self::new(backend::system(), service)
    }

    fn account(set_id: &str, key: &str) -> String {
        format!("{set_id}/{key}")
    }

    /// What the keychain says when asked about an account that holds nothing.
    pub fn state(&self) -> EnvKeychain {
        match self.backend.get(&self.service, "probe") {
            Ok(_) | Err(Failure::Other) => EnvKeychain::Ok,
            Err(Failure::Unusable) => EnvKeychain::Unavailable,
            Err(Failure::Blocked) => EnvKeychain::Blocked,
        }
    }

    /// Whether a non-empty value is saved. Never the value, its length or any part of it.
    pub fn present(&self, set_id: &str, key: &str) -> Result<bool, EnvError> {
        match self.backend.get(&self.service, &Self::account(set_id, key)) {
            Ok(Some(bytes)) => Ok(!bytes.is_empty()),
            Ok(None) => Ok(false),
            Err(failure) => Err(map(failure)),
        }
    }

    /// Saves or replaces a value. The caller drops its copy right after.
    pub fn save(&self, set_id: &str, key: &str, value: &str) -> Result<(), EnvError> {
        if value.is_empty() || value.len() > MAX_VALUE_BYTES || value.contains('\0') {
            return Err(EnvError::InvalidValue);
        }
        self.backend
            .set(&self.service, &Self::account(set_id, key), value.as_bytes())
            .map_err(map)
    }

    /// Deletes one item. Fine when there is none.
    pub fn remove(&self, set_id: &str, key: &str) -> Result<(), EnvError> {
        self.backend
            .delete(&self.service, &Self::account(set_id, key))
            .map_err(map)
    }

    /// Deletes the items of these keys, stopping at the first that cannot be removed so the caller can leave its rows as they were.
    pub fn remove_all(&self, set_id: &str, keys: &[String]) -> Result<(), EnvError> {
        keys.iter().try_for_each(|key| self.remove(set_id, key))
    }

    /// The saved values of these keys, for one workflow run's environment. A key with nothing saved is left out. This is the only
    /// way a value leaves the keychain.
    pub fn spawn_values(&self, set_id: &str, keys: &[String]) -> Result<Vec<EnvSecret>, EnvError> {
        let mut out = Vec::new();
        for key in keys {
            match self.backend.get(&self.service, &Self::account(set_id, key)) {
                Ok(Some(bytes)) if !bytes.is_empty() => {
                    let value = String::from_utf8(bytes.to_vec()).map_err(|_| EnvError::Failed)?;
                    out.push(EnvSecret {
                        key: key.clone(),
                        value: Zeroizing::new(value),
                    });
                }
                Ok(_) => {}
                Err(failure) => return Err(map(failure)),
            }
        }
        Ok(out)
    }
}

impl std::fmt::Debug for EnvSets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvSets")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}
