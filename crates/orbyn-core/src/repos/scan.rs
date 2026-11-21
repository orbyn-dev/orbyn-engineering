//! Folder scan and `.code-workspace` import (docs/multi-repo.md). Reads only; nothing is added or trusted.

use crate::error::{CoreError, Result};
use orbyn_contracts::{RepoScan, ScanRefusal, ScanSource, ScannedFolder};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

const MAX_EXAMINED: usize = 200;
const MAX_RESULTS: usize = 64;
const MAX_WORKSPACE_BYTES: u64 = 256 * 1024;
const MAX_WORKSPACE_FOLDERS: usize = 64;
const SUFFIX: &str = ".code-workspace";

fn is_git(path: &Path) -> bool {
    std::fs::symlink_metadata(path.join(".git")).is_ok()
}

fn display(path: &Path) -> String {
    crate::util::strip_verbatim(&path.to_string_lossy())
}

fn leaf(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| display(path), |name| name.to_string_lossy().into_owned())
}

/// `path` with `.` and `..` folded in, without touching the disk. A `..` that climbs past the start stays, so it can
/// be seen as outside.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// JSON with comments and trailing commas, as a `.code-workspace` file is, as plain JSON text.
pub(crate) fn plain_json(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut in_string, mut escaped) = (0, false, false);
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match (c, chars.get(i + 1)) {
            ('"', _) => {
                in_string = true;
                out.push(c);
                i += 1;
            }
            ('/', Some('/')) => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    // A comma before a closing bracket is dropped.
    let bytes: Vec<char> = out.chars().collect();
    let mut cleaned = String::with_capacity(out.len());
    let (mut in_string, mut escaped) = (false, false);
    for (index, c) in bytes.iter().enumerate() {
        if in_string {
            cleaned.push(*c);
            if escaped {
                escaped = false;
            } else if *c == '\\' {
                escaped = true;
            } else if *c == '"' {
                in_string = false;
            }
            continue;
        }
        if *c == '"' {
            in_string = true;
        }
        if *c == ','
            && bytes[index + 1..]
                .iter()
                .find(|n| !n.is_whitespace())
                .is_some_and(|n| matches!(n, '}' | ']'))
        {
            continue;
        }
        cleaned.push(*c);
    }
    cleaned
}

/// The folders of a workspace file, each resolved against the file's own folder.
fn from_workspace(file: &Path, known: &HashSet<String>) -> Result<Vec<ScannedFolder>> {
    let meta =
        std::fs::metadata(file).map_err(|_| CoreError::Invalid("That file is gone.".into()))?;
    if meta.len() > MAX_WORKSPACE_BYTES {
        return Err(CoreError::Invalid(
            "That workspace file is too large.".into(),
        ));
    }
    let text = std::fs::read_to_string(file)
        .map_err(|_| CoreError::Invalid("That workspace file is not text.".into()))?;
    let value: Value = serde_json::from_str(&plain_json(&text))
        .map_err(|_| CoreError::Invalid("That workspace file is not valid.".into()))?;
    let entries = value
        .get("folders")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Invalid("That workspace file lists no folders.".into()))?;
    let base = file
        .parent()
        .ok_or_else(|| CoreError::Invalid("That workspace file has no folder.".into()))?;
    let mut out = Vec::new();
    for entry in entries.iter().take(MAX_WORKSPACE_FOLDERS) {
        let Some(raw) = entry.get("path").and_then(Value::as_str) else {
            continue;
        };
        let given = Path::new(raw);
        let looks_foreign = raw.contains('\\') || raw.contains(':') || raw.starts_with("//");
        let joined = if given.is_absolute() {
            given.to_path_buf()
        } else {
            base.join(given)
        };
        let wanted = lexical(&joined);
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .map_or_else(|| leaf(&wanted), str::to_owned);
        let mut folder = ScannedFolder {
            name,
            path: display(&wanted),
            is_git: false,
            source: ScanSource::CodeWorkspace,
            in_project: false,
            refused: None,
        };
        if looks_foreign || !wanted.starts_with(base) {
            folder.refused = Some(ScanRefusal::Outside);
        } else {
            match crate::util::canonicalize(&wanted) {
                Err(_) => folder.refused = Some(ScanRefusal::Missing),
                Ok(real) if !real.starts_with(base) => folder.refused = Some(ScanRefusal::Outside),
                Ok(real) if !real.is_dir() => folder.refused = Some(ScanRefusal::NotDirectory),
                Ok(real) => {
                    folder.path = display(&real);
                    folder.is_git = is_git(&real);
                    folder.in_project = known.contains(&folder.path);
                }
            }
        }
        out.push(folder);
    }
    Ok(out)
}

/// The `.code-workspace` file that is the only one at the top of `dir`.
fn only_workspace_file(dir: &Path) -> Option<PathBuf> {
    let mut found = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(SUFFIX)
            && entry.file_type().is_ok_and(|kind| kind.is_file())
        {
            if found.is_some() {
                return None;
            }
            found = Some(entry.path());
        }
    }
    found
}

/// What is at `path` (already canonical): the folders in the workspace file it is or holds, or its child folders.
pub(crate) fn scan(path: &Path, known: &HashSet<String>) -> Result<RepoScan> {
    if path.is_file() {
        if !leaf(path).ends_with(SUFFIX) {
            return Err(CoreError::Invalid(
                "Choose a folder or a .code-workspace file.".into(),
            ));
        }
        let folders = from_workspace(path, known)?;
        return Ok(RepoScan {
            path: display(path.parent().unwrap_or(path)),
            is_git: path.parent().is_some_and(is_git),
            code_workspace_file: Some(leaf(path)),
            folders,
            truncated: false,
        });
    }
    if let Some(file) = only_workspace_file(path) {
        let folders = from_workspace(&file, known)?;
        return Ok(RepoScan {
            path: display(path),
            is_git: is_git(path),
            code_workspace_file: Some(leaf(&file)),
            folders,
            truncated: false,
        });
    }
    let mut names: Vec<(String, PathBuf)> = Vec::new();
    let mut examined = 0;
    let mut truncated = false;
    for entry in std::fs::read_dir(path)
        .map_err(|_| CoreError::Invalid("That folder cannot be read.".into()))?
        .flatten()
    {
        examined += 1;
        if examined > MAX_EXAMINED {
            truncated = true;
            break;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_real_dir = entry
            .file_type()
            .is_ok_and(|kind| kind.is_dir() && !kind.is_symlink());
        if name.starts_with('.') || !is_real_dir {
            continue;
        }
        names.push((name, entry.path()));
    }
    names.sort();
    if names.len() > MAX_RESULTS {
        names.truncate(MAX_RESULTS);
        truncated = true;
    }
    let folders = names
        .into_iter()
        .map(|(name, child)| {
            let real = crate::util::canonicalize(&child).unwrap_or(child);
            let text = display(&real);
            ScannedFolder {
                name,
                is_git: is_git(&real),
                in_project: known.contains(&text),
                path: text,
                source: ScanSource::Child,
                refused: None,
            }
        })
        .collect();
    Ok(RepoScan {
        path: display(path),
        is_git: is_git(path),
        code_workspace_file: None,
        folders,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> crate::testing::TempDir {
        crate::testing::TempDir::new()
    }

    fn repo(path: &Path) {
        std::fs::create_dir_all(path.join(".git")).unwrap();
    }

    #[test]
    fn jsonc_loses_its_comments_and_trailing_commas_but_not_strings() {
        let text = "{ // c\n \"a\": \"x // not a comment, ]\", /* b */ \"folders\": [ {\"path\": \"p\"}, ], }";
        let value: Value = serde_json::from_str(&plain_json(text)).unwrap();
        assert_eq!(value["a"], "x // not a comment, ]");
        assert_eq!(value["folders"][0]["path"], "p");
    }

    #[test]
    fn a_scan_lists_child_folders_only_one_level_down_and_follows_no_link() {
        let root = dir();
        let root = crate::util::canonicalize(root.path()).unwrap();
        repo(&root.join("a"));
        std::fs::create_dir_all(root.join("b/deeper")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("file"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/", root.join("link")).unwrap();
        let known = HashSet::from([display(&root.join("b"))]);
        let found = scan(&root, &known).unwrap();
        let names: Vec<_> = found.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert!(found.folders[0].is_git && !found.folders[1].is_git);
        assert!(found.folders[1].in_project && !found.folders[0].in_project);
        assert!(!found.truncated && found.code_workspace_file.is_none());
    }

    #[test]
    fn a_scan_is_capped() {
        let root = dir();
        let root = crate::util::canonicalize(root.path()).unwrap();
        for n in 0..70 {
            std::fs::create_dir(root.join(format!("d{n:03}"))).unwrap();
        }
        let found = scan(&root, &HashSet::new()).unwrap();
        assert_eq!(found.folders.len(), MAX_RESULTS);
        assert!(found.truncated);
    }

    #[test]
    fn a_workspace_file_lists_its_folders_and_refuses_what_leaves_its_folder() {
        let root = dir();
        let root = crate::util::canonicalize(root.path()).unwrap();
        repo(&root.join("app"));
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("note.txt"), "x").unwrap();
        std::fs::write(
            root.join("w.code-workspace"),
            r#"{ // mine
              "folders": [
                { "path": "app" }, { "path": "./docs", "name": "Docs" }, { "path": "gone" },
                { "path": "note.txt" }, { "path": "../outside" }, { "path": "/etc" },
                { "path": "app/../../up" }, { "path": "C:\\x" },
              ],
              "settings": { "task": "rm -rf /" },
            }"#,
        )
        .unwrap();
        let by_file = scan(&root.join("w.code-workspace"), &HashSet::new()).unwrap();
        let by_folder = scan(&root, &HashSet::new()).unwrap();
        assert_eq!(by_file.folders, by_folder.folders);
        assert_eq!(
            by_folder.code_workspace_file.as_deref(),
            Some("w.code-workspace")
        );
        let state: Vec<_> = by_folder
            .folders
            .iter()
            .map(|f| (f.name.as_str(), f.refused))
            .collect();
        assert_eq!(
            state,
            [
                ("app", None),
                ("Docs", None),
                ("gone", Some(ScanRefusal::Missing)),
                ("note.txt", Some(ScanRefusal::NotDirectory)),
                ("outside", Some(ScanRefusal::Outside)),
                ("etc", Some(ScanRefusal::Outside)),
                ("up", Some(ScanRefusal::Outside)),
                ("C:\\x", Some(ScanRefusal::Outside)),
            ]
        );
        assert!(by_folder.folders[0].is_git);
    }

    #[test]
    fn two_workspace_files_or_a_bad_one_do_not_guess() {
        let root = dir();
        let root = crate::util::canonicalize(root.path()).unwrap();
        std::fs::write(root.join("a.code-workspace"), "{}").unwrap();
        std::fs::write(root.join("b.code-workspace"), "{}").unwrap();
        // Two files: the folder is scanned as folders.
        assert!(
            scan(&root, &HashSet::new())
                .unwrap()
                .code_workspace_file
                .is_none()
        );
        assert!(scan(&root.join("a.code-workspace"), &HashSet::new()).is_err());
        std::fs::write(root.join("c.txt"), "x").unwrap();
        assert!(scan(&root.join("c.txt"), &HashSet::new()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_workspace_folder_is_outside() {
        let root = dir();
        let root = crate::util::canonicalize(root.path()).unwrap();
        let elsewhere = dir();
        std::os::unix::fs::symlink(elsewhere.path(), root.join("sneak")).unwrap();
        std::fs::write(
            root.join("w.code-workspace"),
            r#"{ "folders": [ { "path": "sneak" } ] }"#,
        )
        .unwrap();
        let found = scan(&root, &HashSet::new()).unwrap();
        assert_eq!(found.folders[0].refused, Some(ScanRefusal::Outside));
    }
}
