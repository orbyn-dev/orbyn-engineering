//! The data folder's private files: the account key-store marker, the sealed
//! snapshot and the local service token. `WriteScope` refuses them for writes
//! and checkout reads. The approval gate also refuses an action whose text
//! names one; that text check is best effort and is not a boundary.

use std::path::Path;

use crate::governance::approvals::resolve_existing;

/// The local API's service token file.
const SERVICE_FILE: &str = "service.json";
const ACCOUNT_PREFIX: &str = "account-";
const ACCOUNT_SUFFIXES: [&str; 2] = [".store", ".sealed"];

#[cfg(windows)]
const SEPARATORS: [char; 2] = ['/', '\\'];
#[cfg(not(windows))]
const SEPARATORS: [char; 1] = ['/'];

/// Case folding for the platforms whose filesystems and paths are case-insensitive.
fn folded(text: &str) -> String {
    #[cfg(any(target_os = "macos", windows))]
    {
        text.to_lowercase()
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        text.to_owned()
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(any(target_os = "macos", windows))]
    {
        folded(&left.to_string_lossy()) == folded(&right.to_string_lossy())
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        left == right
    }
}

fn protected_name(name: &str) -> bool {
    let name = folded(name);
    name == SERVICE_FILE
        || (name.starts_with(ACCOUNT_PREFIX)
            && ACCOUNT_SUFFIXES.iter().any(|suffix| name.ends_with(suffix)))
}

fn without_trailing_separator(path: &str) -> &str {
    #[cfg(windows)]
    {
        path.trim_end_matches(['/', '\\'])
    }
    #[cfg(not(windows))]
    {
        path.trim_end_matches('/')
    }
}

/// True when `path` is one of `data_dir`'s protected files.
///
/// Both sides are resolved like a write scope: `.` and `..` are folded, and the
/// longest existing prefix has its links followed. A path that does not exist
/// yet is judged by its resolved parent and its file name. A protected file is
/// a direct child of the data folder, so a file with the same name deeper in the
/// workspace is not protected.
pub fn is_protected(data_dir: &Path, path: &Path) -> bool {
    let (Some(path), Some(data_dir)) = (resolve_existing(path), resolve_existing(data_dir)) else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    same_path(parent, &data_dir)
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(protected_name)
}

/// True when `text` appears to name one of `data_dir`'s protected files.
///
/// This is a best-effort text check, not a boundary. It catches the data
/// folder's path followed by a protected name, `service.json` together with the
/// data folder's path, and the bare account file names; an indirect spelling
/// (a shell variable, a built path) still gets past it.
pub fn names_protected(data_dir: &Path, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let data_dir = resolve_existing(data_dir).unwrap_or_else(|| data_dir.to_path_buf());
    let text = folded(text);
    let data_dir = folded(data_dir.to_string_lossy().as_ref());
    names_protected_file(&data_dir, &text)
        || (text.contains(SERVICE_FILE) && text.contains(&data_dir))
        || names_bare_account_file(&text)
}

/// The data folder's path followed by a separator and a protected file name.
fn names_protected_file(data_dir: &str, text: &str) -> bool {
    let trimmed = without_trailing_separator(data_dir);
    let directory = if trimmed.is_empty() {
        data_dir
    } else {
        trimmed
    };
    for separator in SEPARATORS {
        let prefix = format!("{directory}{separator}");
        let mut from = 0;
        while let Some(at) = text.get(from..).and_then(|rest| rest.find(&prefix)) {
            let name_at = from + at + prefix.len();
            let name = text
                .get(name_at..)
                .map(name_before_boundary)
                .unwrap_or_default();
            if protected_name(name) {
                return true;
            }
            from = name_at;
        }
    }
    false
}

/// A bare `account-keys-<profile>.store` or `account-snapshot-<profile>.sealed`.
fn names_bare_account_file(text: &str) -> bool {
    for prefix in ["account-keys-", "account-snapshot-"] {
        let mut from = 0;
        while let Some(at) = text.get(from..).and_then(|rest| rest.find(prefix)) {
            let start = from + at + prefix.len();
            let name = text
                .get(start..)
                .map(name_before_boundary)
                .unwrap_or_default();
            let profile = name
                .strip_suffix(".store")
                .or_else(|| name.strip_suffix(".sealed"));
            if profile.is_some_and(|profile| !profile.is_empty()) {
                return true;
            }
            from = start;
        }
    }
    false
}

/// The file name at the start of `text`, up to the first character that ends a
/// name in a path, a quoted string or a shell command.
fn name_before_boundary(text: &str) -> &str {
    let end = text.find(boundary).unwrap_or(text.len());
    &text[..end]
}

fn boundary(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '/' | '\\'
                | '"'
                | '\''
                | '`'
                | '<'
                | '>'
                | '|'
                | ';'
                | ','
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | ':'
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;
    use std::path::PathBuf;

    fn data_dir() -> (TempDir, PathBuf) {
        let dir = TempDir::new();
        let data = dir.path().join("data");
        std::fs::create_dir(&data).unwrap();
        (dir, data)
    }

    #[test]
    fn direct_account_and_service_files_are_protected() {
        let (_dir, data) = data_dir();
        for name in [
            "account-keys-default.store",
            "account-snapshot-default.sealed",
            "account-other.store",
            "account-other.sealed",
            "service.json",
        ] {
            assert!(is_protected(&data, &data.join(name)), "{name}");
        }
        assert!(is_protected(&data, &data.join("account-keys-new.store")));
        std::fs::create_dir_all(data.join("workspace")).unwrap();
        assert!(is_protected(
            &data,
            &data.join("workspace/../account-keys-default.store")
        ));
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn protected_names_fold_case_on_macos_and_windows() {
        let (_dir, data) = data_dir();
        assert!(is_protected(
            &data,
            &data.join("ACCOUNT-KEYS-DEFAULT.STORE")
        ));
        assert!(is_protected(&data, &data.join("SERVICE.JSON")));
        assert!(names_protected(
            &data,
            &format!("cat {}/ACCOUNT-KEYS-DEFAULT.STORE", data.to_string_lossy())
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_a_protected_file_is_protected() {
        let (dir, data) = data_dir();
        let target = data.join("account-keys-default.store");
        std::fs::write(&target, b"key store marker").unwrap();
        let link = dir.path().join("keys-link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(is_protected(&data, &link));
        let inside = data.join("keys-link");
        std::os::unix::fs::symlink(&target, &inside).unwrap();
        assert!(is_protected(&data, &inside));
    }

    #[test]
    fn nested_files_and_neighbours_are_not_protected() {
        let (_dir, data) = data_dir();
        let nested = data.join("workspace/repo/p");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("account-keys-x.store"), b"nested").unwrap();
        assert!(!is_protected(&data, &nested.join("account-keys-x.store")));
        assert!(!is_protected(&data, &data.join("accounts.txt")));
        assert!(!is_protected(&data, &data.join("orbyn.sqlite3")));
    }

    #[test]
    fn text_names_are_caught_where_the_action_cannot_be_resolved() {
        let (_dir, data) = data_dir();
        let folder = data.to_string_lossy();
        for name in [
            "account-keys-default.store",
            "account-snapshot-default.sealed",
            "service.json",
        ] {
            assert!(
                names_protected(&data, &format!("cat {folder}/{name}")),
                "{name}"
            );
        }
        assert!(names_protected(
            &data,
            &format!("{folder} holds service.json")
        ));
        assert!(names_protected(&data, "cat account-keys-default.store"));
        assert!(names_protected(&data, "rm account-snapshot-x.sealed"));
        assert!(!names_protected(
            &data,
            &format!("cat {folder}/workspace/repo/p/notes.txt")
        ));
        assert!(!names_protected(&data, "cat service.json"));
        assert!(!names_protected(
            &data,
            "cat account-keys-default.store.bak"
        ));
        assert!(!names_protected(&data, "account-keys-.store"));
    }
}
