//! The path guard of the explorer. A client names a worktree by id and a path inside
//! it; nothing else decides where a read lands.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::error::{CoreError, Result};
use crate::governance::approvals::resolve_existing;
use crate::project::files::{names_git, opened_path};
use crate::project::outside_files::within;

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
        || part != trimmed
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
    pub fn with_library(root: &Path, data: &Path, library: Option<PathBuf>) -> Result<Self> {
        let library_repo = library
            .and_then(|library| resolve_existing(&library))
            .filter(|library| library.starts_with(root))
            .map(|library| library.join("repo"));
        Self::new(root, data, library_repo)
    }

    pub fn new(root: &Path, data: &Path, library_repo: Option<PathBuf>) -> Result<Self> {
        let expected = root;
        let root =
            crate::util::canonicalize(root).map_err(|_| CoreError::NotFound("worktree".into()))?;
        if root != expected {
            return Err(refused());
        }
        // Missing protected folders remain protected against creation too.
        let data = resolve_existing(data);
        Ok(Self {
            data,
            excluded: library_repo
                .and_then(|path| resolve_existing(&path))
                .into_iter()
                .collect(),
            root,
        })
    }

    /// True when the real location may be read.
    fn allows(&self, real: &Path) -> bool {
        real.strip_prefix(&self.root)
            .is_ok_and(|inside| !names_git(inside))
            && !self.data.as_ref().is_some_and(|data| within(real, data))
            && !self.excluded.iter().any(|folder| within(real, folder))
    }

    /// True when a listing may show the entry `name` of the folder `dir`.
    pub fn lists(&self, dir: &Path, name: &str) -> bool {
        !reserved(name, false) && self.allows(&dir.join(name))
    }

    /// Opens the folder `real` without following a link at the last step and checks the
    /// path the handle names. Hold it while the folder is read and pass it to
    /// [`Guard::confirm_dir`] afterwards.
    pub fn open_dir(&self, real: &Path) -> Result<std::fs::File> {
        self.open_directory(real, false)
    }

    pub fn write_dir(&self, real: &Path) -> Result<std::fs::File> {
        self.open_directory(real, true)
    }

    fn open_directory(&self, real: &Path, writing: bool) -> Result<std::fs::File> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            let _ = writing;
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
            if writing {
                // Keep every ancestor in place until the mutation ends.
                options.share_mode(0x0000_0001 | 0x0000_0002);
            }
        }
        let handle = options.open(real).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CoreError::NotFound("folder".into())
            } else {
                refused()
            }
        })?;
        if writing {
            self.confirm_write_dir(&handle, real)?;
        }
        self.confirm_dir(&handle, real)?;
        Ok(handle)
    }

    pub fn confirm_write_dir(&self, handle: &std::fs::File, real: &Path) -> Result<()> {
        let meta = handle.metadata()?;
        if !meta.is_dir() {
            return Err(refused());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(refused());
            }
        }
        // Write authority fails closed if the platform cannot identify the handle.
        let opened = opened_path(handle).map_err(|_| refused())?;
        if !self.allows(&opened)
            || crate::util::canonicalize(real).map_err(|_| refused())? != opened
        {
            return Err(refused());
        }
        self.confirm_dir(handle, real)
    }

    /// A mutation may not move an ancestor of excluded application/library data.
    pub fn mutation_path(&self, raw: &str) -> Result<Resolved> {
        let resolved = self.resolve(raw)?;
        if resolved.rel.is_empty()
            || self
                .data
                .iter()
                .chain(&self.excluded)
                .any(|path| within(path, &resolved.real))
        {
            return Err(refused());
        }
        Ok(resolved)
    }

    pub fn root(&self) -> &Path {
        &self.root
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
            "ordinary.",
            "ordinary ",
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

    #[test]
    fn missing_exclusions_and_their_ancestors_cannot_be_created_or_moved() {
        let dir = crate::testing::TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        std::fs::create_dir(root.join("parent")).unwrap();
        let guard = Guard::new(&root, &root.join("parent/data"), Some(root.join("repo"))).unwrap();
        for path in [
            "repo",
            "repo/new",
            "parent/data",
            "parent/data/new",
            "parent",
        ] {
            assert!(guard.mutation_path(path).is_err(), "{path}");
        }
        assert!(guard.mutation_path("normal").is_ok());
        let library = root.join("library");
        std::fs::create_dir(&library).unwrap();
        let current = Guard::with_library(&root, &root.join("parent/data"), Some(library)).unwrap();
        assert!(current.mutation_path("library/repo").is_err());
        assert!(current.mutation_path("library").is_err());
        assert!(current.mutation_path("other/repo").is_ok());
        let missing = Guard::with_library(
            &root,
            &root.join("parent/data"),
            Some(root.join("absent-library")),
        )
        .unwrap();
        assert!(missing.mutation_path("absent-library").is_err());
        assert!(missing.mutation_path("absent-library/repo").is_err());
        #[cfg(any(windows, target_os = "macos"))]
        {
            for path in ["REPO", "PARENT/DATA/new"] {
                assert!(guard.mutation_path(path).is_err(), "{path}");
            }
            assert!(missing.mutation_path("ABSENT-LIBRARY/REPO").is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn write_handles_must_name_directories() {
        let dir = crate::testing::TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("file"), "content").unwrap();
        let guard = Guard::new(&root, &root.join("data"), None).unwrap();
        let file = std::fs::File::open(root.join("file")).unwrap();
        assert!(guard.confirm_write_dir(&file, &root.join("file")).is_err());
        let held = guard.write_dir(&root).unwrap();
        assert!(guard.confirm_write_dir(&held, &root).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn write_handles_refuse_junctions_even_when_the_target_is_inside() {
        let dir = crate::testing::TempDir::new();
        let root = crate::util::canonicalize(dir.path()).unwrap();
        std::fs::create_dir(root.join("inside")).unwrap();
        // Junction creation needs no symbolic-link privilege on supported Windows.
        // Both paths come exclusively from this disposable fixture.
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(root.join("junction"))
            .arg(root.join("inside"))
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let guard = Guard::new(&root, &root.join("data"), None).unwrap();
        assert!(guard.write_dir(&root.join("junction")).is_err());
        assert!(guard.write_dir(&root.join("inside")).is_ok());
    }
}
