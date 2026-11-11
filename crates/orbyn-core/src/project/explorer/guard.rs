//! The path guard of the explorer. A client names a worktree by id and a path inside
//! it; nothing else decides where a read lands.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::governance::approvals::resolve_existing;
use crate::project::files::{names_git, opened_path};

const MAX_PATH_BYTES: usize = 4096;

fn refused() -> CoreError {
    CoreError::Invalid("That path is outside this worktree.".into())
}

/// Device names Windows opens as a device whatever the extension.
fn windows_device(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            upper
                .strip_prefix(prefix)
                .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
        })
}

/// A name no read may use: Git's folder in any spelling, and on Windows its short
/// name, a stream suffix, trailing dots and spaces that Windows drops, and devices.
fn reserved(part: &str, windows: bool) -> bool {
    if part.eq_ignore_ascii_case(".git") {
        return true;
    }
    if !windows {
        return false;
    }
    let trimmed = part.trim_end_matches(['.', ' ']);
    part.contains(':')
        || trimmed.eq_ignore_ascii_case(".git")
        || trimmed.eq_ignore_ascii_case("git~1")
        || windows_device(part)
}

/// The path a client sent, made plain and checked by its spelling alone: no
/// absolute or drive path, no `..`, no backslash, no control character and no part
/// that names Git's folder. `a//b` and `./a` are `a/b` and `a`. The empty path is the
/// root. The query string is decoded once by the service, so `%2e%2e` arrives as `..`
/// and is refused here; a doubly encoded one is only an odd file name.
pub(super) fn clean(raw: &str, windows: bool) -> Result<String> {
    if raw.len() > MAX_PATH_BYTES
        || raw.chars().any(|c| c.is_control() || c == '\\')
        || raw.starts_with('/')
        || Path::new(raw).has_root()
        || Path::new(raw).is_absolute()
    {
        return Err(refused());
    }
    let mut parts = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(refused()),
            part if reserved(part, windows) => return Err(refused()),
            part => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

/// The folders a worktree read never reaches, canonicalized.
pub(super) struct Guard {
    root: PathBuf,
    /// Orbyn's own data folder, when it sits inside the worktree.
    data: Option<PathBuf>,
    /// The library's `repo/`, when the worktree is the library.
    excluded: Vec<PathBuf>,
}

/// A path that cleared the guard.
pub(super) struct Resolved {
    /// What the client sent, cleaned.
    pub rel: String,
    /// Where it really lands, links followed. It may not exist.
    pub real: PathBuf,
}

impl Guard {
    pub fn new(root: &Path, data: &Path, library_repo: Option<PathBuf>) -> Result<Self> {
        let root =
            crate::util::canonicalize(root).map_err(|_| CoreError::NotFound("worktree".into()))?;
        let data = crate::util::canonicalize(data).ok();
        Ok(Self {
            data,
            excluded: library_repo
                .and_then(|path| crate::util::canonicalize(&path).ok())
                .into_iter()
                .collect(),
            root,
        })
    }

    /// True when the real location may be read.
    fn allows(&self, real: &Path) -> bool {
        real.strip_prefix(&self.root)
            .is_ok_and(|inside| !names_git(inside))
            && !self
                .data
                .as_ref()
                .is_some_and(|data| real.starts_with(data))
            && !self.excluded.iter().any(|folder| real.starts_with(folder))
    }

    /// True when a listing may show the entry `name` of the folder `dir`.
    pub fn lists(&self, dir: &Path, name: &str) -> bool {
        !reserved(name, false) && self.allows(&dir.join(name))
    }

    /// Opens the folder `real` without following a link at the last step and checks the
    /// path the handle names. Hold it while the folder is read and pass it to
    /// [`Guard::confirm_dir`] afterwards.
    pub fn open_dir(&self, real: &Path) -> Result<std::fs::File> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let handle = options.open(real).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CoreError::NotFound("folder".into())
            } else {
                refused()
            }
        })?;
        self.confirm_dir(&handle, real)?;
        Ok(handle)
    }

    /// Refuses when the folder the handle holds is no longer inside the worktree, or is
    /// no longer the one `real` names now. Called before and after the entries are read,
    /// which narrows a swap of a parent folder for a link to a window no filesystem lets
    /// a path-based read close.
    pub fn confirm_dir(&self, handle: &std::fs::File, real: &Path) -> Result<()> {
        if let Ok(opened) = opened_path(handle)
            && !self.allows(&opened)
        {
            return Err(refused());
        }
        #[cfg(not(unix))]
        let _ = real;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let held = handle.metadata()?;
            match std::fs::metadata(real) {
                Ok(named) if (named.dev(), named.ino()) == (held.dev(), held.ino()) => {}
                _ => return Err(refused()),
            }
        }
        Ok(())
    }

    pub fn resolve(&self, raw: &str) -> Result<Resolved> {
        let rel = clean(raw, cfg!(windows))?;
        let real = resolve_existing(&self.root.join(&rel)).ok_or_else(refused)?;
        if !self.allows(&real) {
            return Err(refused());
        }
        Ok(Resolved { rel, real })
    }

    /// Opens the file without following a link at the last step, so a name swapped
    /// for a link after the check cannot lead elsewhere, then checks the path the
    /// handle names where the system can say. Reads at most `cap` bytes; `None` means
    /// the file is larger. A folder, device, pipe or socket is refused.
    pub fn read(&self, raw: &str, cap: u64) -> Result<(Resolved, u64, Option<Vec<u8>>)> {
        let resolved = self.resolve(raw)?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let file = options.open(&resolved.real).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CoreError::NotFound("file".into())
            } else {
                refused()
            }
        })?;
        let meta = file.metadata()?;
        if !meta.is_file() {
            return Err(CoreError::Invalid("That path is not a file.".into()));
        }
        if let Ok(opened) = opened_path(&file)
            && !self.allows(&opened)
        {
            return Err(refused());
        }
        let size = meta.len();
        if size > cap {
            return Ok((resolved, size, None));
        }
        let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
        file.take(cap + 1).read_to_end(&mut bytes)?;
        // A file that grew past the cap while it was read counts as too large too.
        if bytes.len() as u64 > cap {
            return Ok((resolved, bytes.len() as u64, None));
        }
        Ok((resolved, size, Some(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelling_alone_refuses_traversal_and_git() {
        for bad in [
            "..",
            "../x",
            "a/../b",
            "a/..",
            "/etc/passwd",
            "/",
            "a\\b",
            "..\\x",
            ".git",
            ".GIT/config",
            "src/.Git/hooks",
            "a\0b",
            "a\nb",
        ] {
            assert!(clean(bad, false).is_err(), "{bad:?}");
        }
        for bad in [
            "C:/x",
            "C:x",
            "a/.git.",
            "a/.git ",
            "GIT~1/config",
            "a/b:stream",
            "NUL",
            "a/con.txt",
            "COM1",
        ] {
            assert!(clean(bad, true).is_err(), "{bad:?}");
        }
        assert_eq!(clean("", false).unwrap(), "");
        assert_eq!(clean("./a//b/", false).unwrap(), "a/b");
        assert_eq!(clean("a:b.rs", false).unwrap(), "a:b.rs");
        assert_eq!(clean("%2e%2e/x", false).unwrap(), "%2e%2e/x");
        assert!(clean(&"a/".repeat(3000), false).is_err());
    }

    #[test]
    fn a_device_name_needs_the_whole_stem() {
        assert!(windows_device("NUL"));
        assert!(windows_device("com1.txt"));
        assert!(!windows_device("console"));
        assert!(!windows_device("com10"));
    }
}
