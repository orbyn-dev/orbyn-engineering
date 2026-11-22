//! The real Codex home, the auth rule a Codex worker must pass, and the private home one
//! crew task gets.
//!
//! Codex keeps its saved conversations and its login under `CODEX_HOME` (or `~/.codex`). A
//! worker must not read that folder, so its engine gets a home of its own with only a copy of
//! `auth.json`; the real one is a closed folder in the profile. The copy is made only from a
//! regular file (never a link), and only when the login is an API key: a ChatGPT sign-in would
//! hand the worker the person's own session, which is a separate login for later work
//! (`docs/crew.md`).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};

/// The exact line a person sees when the Codex login is not an API key.
pub(crate) const AUTH_REFUSED: &str =
    "Codex workers need an API key; your ChatGPT sign-in would be shared with the worker.";
const AUTH_MISSING: &str = "Codex workers need an API key; no Codex sign-in was found.";
const AUTH_UNREADABLE: &str = "Codex workers need an API key; the Codex sign-in could not be read.";

/// An auth document bigger than this is not a login file; it is refused, not read into memory.
const AUTH_MAX_BYTES: u64 = 1024 * 1024;

/// What the real `auth.json` says the sign-in is, read from `auth_mode` and the presence of
/// the API-key field only. Nothing else in the document is kept or logged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthKind {
    ApiKey,
    ChatGpt,
    Unknown,
}

/// Only these two fields are deserialised; every token, plan or account value in the
/// document is ignored rather than kept.
#[derive(serde::Deserialize)]
struct AuthDocument {
    #[serde(default)]
    auth_mode: Option<String>,
    #[serde(default, rename = "OPENAI_API_KEY")]
    api_key: Option<String>,
}

/// The login a worker's home gets: only the mode and the API key. A real `auth.json` that is in
/// API-key mode can still hold the person's ChatGPT `tokens` from an earlier sign-in; none of
/// that is copied.
pub(crate) fn worker_auth(document: &[u8]) -> Result<Vec<u8>> {
    check_auth(document)?;
    let parsed: AuthDocument = serde_json::from_slice(document)
        .map_err(|_| CoreError::EngineUnavailable(AUTH_UNREADABLE.to_owned()))?;
    serde_json::to_vec(&serde_json::json!({
        "auth_mode": "apikey",
        "OPENAI_API_KEY": parsed.api_key,
    }))
    .map_err(|_| CoreError::EngineUnavailable(AUTH_UNREADABLE.to_owned()))
}

pub(crate) fn auth_kind(document: &[u8]) -> AuthKind {
    let Ok(document) = serde_json::from_slice::<AuthDocument>(document) else {
        return AuthKind::Unknown;
    };
    match document.auth_mode.as_deref() {
        Some("apikey")
            if document
                .api_key
                .as_deref()
                .is_some_and(|key| !key.is_empty()) =>
        {
            AuthKind::ApiKey
        }
        Some("chatgpt") => AuthKind::ChatGpt,
        _ => AuthKind::Unknown,
    }
}

pub(crate) fn check_auth(document: &[u8]) -> Result<()> {
    match auth_kind(document) {
        AuthKind::ApiKey => Ok(()),
        AuthKind::ChatGpt | AuthKind::Unknown => {
            Err(CoreError::EngineUnavailable(AUTH_REFUSED.to_owned()))
        }
    }
}

/// The core process's real Codex home: `CODEX_HOME` when set, else the person's `~/.codex`.
/// It must exist and resolve; nothing is guessed.
pub(crate) fn real_codex_home() -> Result<PathBuf> {
    let named = match std::env::var_os("CODEX_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => crate::util::home_dir()
            .or_else(account_home)
            .ok_or_else(|| CoreError::EngineUnavailable(AUTH_MISSING.to_owned()))?
            .join(".codex"),
    };
    if !named.is_absolute() {
        return Err(CoreError::EngineUnavailable(AUTH_MISSING.to_owned()));
    }
    let real = std::fs::canonicalize(&named)
        .map_err(|_| CoreError::EngineUnavailable(AUTH_MISSING.to_owned()))?;
    if !real.is_dir() {
        return Err(CoreError::EngineUnavailable(AUTH_MISSING.to_owned()));
    }
    Ok(real)
}

/// The effective home first, then the other Codex homes this account may keep. `HOME` and the
/// account's own home from the user database can differ, and a service started with another
/// `HOME` must not leave the person's own saved conversations open.
pub(crate) fn real_codex_homes() -> Result<(PathBuf, Vec<PathBuf>)> {
    let effective = real_codex_home()?;
    let mut others = Vec::new();
    for home in [crate::util::home_dir(), account_home()] {
        let Some(home) = home else {
            continue;
        };
        let named = home.join(".codex");
        let Ok(real) = std::fs::canonicalize(&named) else {
            continue;
        };
        if real != effective && !others.contains(&real) {
            others.push(real);
        }
    }
    Ok((effective, others))
}

#[cfg(target_os = "macos")]
fn account_home() -> Option<PathBuf> {
    super::macos::account_home()
}

#[cfg(not(target_os = "macos"))]
fn account_home() -> Option<PathBuf> {
    crate::util::home_dir()
}

/// The task id as one folder name. On macOS and Linux the id itself is a legal name; Windows
/// cannot use `:`, so there it is encoded as `%3A`. Only letters, digits, `-`, `_` and `:` pass,
/// so no id can become a path.
pub(crate) fn home_name(task: &str) -> Result<String> {
    if task.is_empty() || task.len() > 256 || task.starts_with('.') {
        return Err(CoreError::EngineUnavailable(
            "A crew worker could not be restricted: its task id is not usable.".into(),
        ));
    }
    if !task
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
    {
        return Err(CoreError::EngineUnavailable(
            "A crew worker could not be restricted: its task id is not usable.".into(),
        ));
    }
    #[cfg(windows)]
    {
        Ok(task.replace(':', "%3A"))
    }
    #[cfg(not(windows))]
    {
        Ok(task.to_owned())
    }
}

#[cfg(windows)]
fn task_of_name(name: &str) -> Option<String> {
    if name.is_empty() || name.starts_with('.') {
        return None;
    }
    let mut task = String::new();
    let mut characters = name.chars();
    while let Some(character) = characters.next() {
        if character == '%' {
            let rest: String = characters.by_ref().take(2).collect();
            if rest != "3A" {
                return None;
            }
            task.push(':');
        } else if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | ':') {
            task.push(character);
        } else {
            return None;
        }
    }
    (!task.is_empty()).then_some(task)
}

#[cfg(not(windows))]
fn task_of_name(name: &str) -> Option<String> {
    if name.is_empty() || name.starts_with('.') {
        return None;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
        .then(|| name.to_owned())
}

pub(crate) fn homes_root(data: &Path) -> PathBuf {
    data.join("crew").join("codex-homes")
}

pub(crate) fn home_path(data: &Path, task: &str) -> Result<PathBuf> {
    Ok(homes_root(data).join(home_name(task)?))
}

/// Makes the task's private home and copies only `auth.json` into it. The login is checked
/// before any folder is made, so a ChatGPT sign-in leaves no trace behind.
pub(crate) fn prepare_home(data: &Path, task: &str, real_home: &Path) -> Result<PathBuf> {
    let document = worker_auth(&read_auth(real_home)?)?;
    let data = std::fs::canonicalize(data).map_err(|error| {
        CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} did not resolve ({error}).",
            data.display()
        ))
    })?;
    let home = home_path(&data, task)?;
    make_private(&data.join("crew"))?;
    make_private(&data.join("crew").join("codex-homes"))?;
    make_private(&home)?;
    write_auth(&home.join("auth.json"), &document)?;
    let real = std::fs::canonicalize(&home).map_err(|error| {
        CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} did not resolve ({error}).",
            home.display()
        ))
    })?;
    if real != home {
        return Err(CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} goes through a link.",
            home.display()
        )));
    }
    check_private_home(&data, &home, &[])?;
    Ok(home)
}

/// Reads only a regular `auth.json`. A link is refused rather than followed, and the size is
/// bounded before anything is parsed.
fn read_auth(real_home: &Path) -> Result<Vec<u8>> {
    let path = real_home.join("auth.json");
    let meta = std::fs::symlink_metadata(&path)
        .map_err(|_| CoreError::EngineUnavailable(AUTH_MISSING.to_owned()))?;
    if meta.file_type().is_symlink() || !meta.is_file() || meta.len() > AUTH_MAX_BYTES {
        return Err(CoreError::EngineUnavailable(AUTH_UNREADABLE.to_owned()));
    }
    let mut file = open_read_only(&path)
        .map_err(|_| CoreError::EngineUnavailable(AUTH_UNREADABLE.to_owned()))?;
    let mut document = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    file.take(AUTH_MAX_BYTES + 1)
        .read_to_end(&mut document)
        .map_err(|_| CoreError::EngineUnavailable(AUTH_UNREADABLE.to_owned()))?;
    if document.len() as u64 > AUTH_MAX_BYTES {
        return Err(CoreError::EngineUnavailable(AUTH_UNREADABLE.to_owned()));
    }
    Ok(document)
}

fn open_read_only(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    options.open(path)
}

fn write_auth(path: &Path, document: &[u8]) -> Result<()> {
    let failed = |why: &str| {
        CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} could not be written ({why}).",
            path.display()
        ))
    };
    if let Ok(meta) = std::fs::symlink_metadata(path)
        && (meta.file_type().is_symlink() || !meta.is_file())
    {
        return Err(failed("not a plain file"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .map_err(|error| failed(&error.to_string()))?;
    file.write_all(document)
        .map_err(|error| failed(&error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path)
            && meta.permissions().mode() & 0o777 != 0o600
            && let Err(error) =
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        {
            return Err(failed(&error.to_string()));
        }
    }
    Ok(())
}

fn make_private(path: &Path) -> Result<()> {
    let failed = |why: &str| {
        CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} {why}.",
            path.display()
        ))
    };
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(failed("is not a private folder"));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                // SAFETY: `geteuid` has no preconditions and cannot fail.
                if meta.uid() != unsafe { libc::geteuid() } {
                    return Err(failed("is not owned by this person"));
                }
                set_private_mode(path).map_err(|error| failed(&error.to_string()))?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder
                .create(path)
                .map_err(|error| failed(&error.to_string()))?;
            #[cfg(unix)]
            set_private_mode(path).map_err(|error| failed(&error.to_string()))?;
            Ok(())
        }
        Err(error) => Err(failed(&format!("could not be read ({error})"))),
    }
}

#[cfg(unix)]
fn set_private_mode(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::metadata(path)?.permissions().mode() & 0o777 != 0o700 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// A CODEX_HOME the driver is about to set must be the private folder under `data`, resolve to
/// itself, and not be the real Codex home (or inside it).
pub(crate) fn check_private_home(data: &Path, home: &Path, closed: &[PathBuf]) -> Result<()> {
    let refuse = |why: &str| {
        CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: its Codex home {why}."
        ))
    };
    let root = std::fs::canonicalize(homes_root(data)).map_err(|_| refuse("is not there"))?;
    let real = std::fs::canonicalize(home).map_err(|_| refuse("is not there"))?;
    if real.as_path() != home {
        return Err(refuse("resolves through a link"));
    }
    if real == root || !real.starts_with(&root) {
        return Err(refuse("is outside its private folder"));
    }
    for folder in closed {
        let folder = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.clone());
        if real == folder || real.starts_with(&folder) || folder.starts_with(&real) {
            return Err(refuse("is the real Codex home"));
        }
    }
    let meta = std::fs::symlink_metadata(home).map_err(|_| refuse("is not there"))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(refuse("is not a folder"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        if meta.uid() != unsafe { libc::geteuid() } {
            return Err(refuse("is not owned by this person"));
        }
    }
    Ok(())
}

/// Whether a folder between `data` and `path` is a link. A deletion never follows one.
fn linked_below(data: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(data) else {
        return true;
    };
    let mut current = data.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => return true,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
            Err(_) => return true,
        }
    }
    false
}

/// Removes one task's home. Never follows a link: a link (or a plain file) is removed itself,
/// and a folder is removed with its contents.
pub(crate) fn remove_codex_home(data: &Path, task: &str) -> std::io::Result<()> {
    let home = home_path(data, task).map_err(|error| std::io::Error::other(error.to_string()))?;
    let parent = home.parent().unwrap_or(data);
    if linked_below(data, parent) {
        return Err(std::io::Error::other(
            "the Codex worker home goes through a link",
        ));
    }
    match std::fs::symlink_metadata(&home) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            std::fs::remove_dir_all(&home)
        }
        Ok(_) => std::fs::remove_file(&home),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Removes every home whose task is not live. `live` is asked once per folder name; an
/// undecodable name or a plain file is stale by definition. Links are removed, never followed.
pub(crate) fn sweep_codex_homes(data: &Path, live: impl Fn(&str) -> bool) -> std::io::Result<()> {
    let root = homes_root(data);
    if linked_below(data, &root) {
        return Err(std::io::Error::other(
            "the Codex worker homes folder goes through a link",
        ));
    }
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let keep = task_of_name(&name).is_some_and(|task| live(&task));
        if keep {
            continue;
        }
        let path = entry.path();
        let removed = match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
                std::fs::remove_dir_all(&path)
            }
            Ok(_) => std::fs::remove_file(&path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
        if let Err(error) = removed
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                %error,
                path = %path.display(),
                "could not remove a stale Codex worker home"
            );
        }
    }
    Ok(())
}
