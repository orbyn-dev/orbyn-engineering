//! Which paths a created check may add (docs/crew/escapes.md, decision 4: "a created check only adds test files, never policy files").
//! The design names no globs, so these are the ones this version uses, and they are the only rule: the core compares the diff of the
//! worker's change against them and refuses anything else, whatever the worker or its brief said.
//!
//! A path is a **test file** when it is relative, has no `..`, `.git`, empty or backslash segment, no glob character (`* ? [ ] { } !`) and no `:` or quote, is not a **policy file** (below), and
//! either a folder in it is `tests`, `test`, `__tests__`, `spec`, `specs` or `e2e`, or its name starts `test_` or holds `.test.`,
//! `.spec.`, `_test.` or `_spec.`. Case is ignored.
//!
//! A **policy file** is never a test file even inside a test folder: anything under `.github/`, `.orbyn/`, `.circleci/`, `.gitlab*`,
//! `.cargo/`, `Jenkinsfile`, the manifests and runners that define what a check command does (`package.json`, `Makefile`, `justfile`,
//! `Taskfile.yml`, `tox.ini`, `noxfile.py`, `pyproject.toml`, `build.rs`, `conftest.py`, `pytest.ini`, `setup.cfg`, `setup.py`,
//! `Cargo.toml`, `Cargo.lock`) and `jest.config.*` and `vitest.config.*`.
//!
//! The worker's change is **only added files**: a modified, deleted, renamed, copied or type-changed path, a symlink, a submodule or
//! more than [`MOST_FILES`] files is refused, and the one path the person proposed must be among them.

/// The most files one check adds.
pub const MOST_FILES: usize = 5;

/// The longest path, in bytes.
pub const PATH_MAX: usize = 200;

fn segments_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= PATH_MAX
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains([':', '*', '?', '[', ']', '{', '}', '!', '"', '\''])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != ".." && !s.eq_ignore_ascii_case(".git"))
}

/// A file that defines what a check command does or how the repository is gated.
pub fn is_policy(path: &str) -> bool {
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    lower.starts_with(".github/")
        || lower.starts_with(".orbyn/")
        || lower.starts_with(".circleci/")
        || lower.starts_with(".gitlab")
        || lower.starts_with(".cargo/")
        || lower.contains("/.cargo/")
        || name == "jenkinsfile"
        || matches!(
            name,
            "package.json"
                | "makefile"
                | "justfile"
                | "taskfile.yml"
                | "tox.ini"
                | "noxfile.py"
                | "pyproject.toml"
                | "build.rs"
                | "conftest.py"
                | "pytest.ini"
                | "setup.cfg"
                | "setup.py"
                | "cargo.toml"
                | "cargo.lock"
        )
        || name.starts_with("jest.config.")
        || name.starts_with("vitest.config.")
}

/// Whether `path` is a test file by the rule in the module notes.
pub fn is_test_path(path: &str) -> bool {
    if !segments_ok(path) || is_policy(path) {
        return false;
    }
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    lower
        .split('/')
        .rev()
        .skip(1)
        .any(|s| matches!(s, "tests" | "test" | "__tests__" | "spec" | "specs" | "e2e"))
        || name.starts_with("test_")
        || [".test.", ".spec.", "_test.", "_spec."]
            .iter()
            .any(|marker| name.contains(marker))
}

/// Why `path` cannot be the new test file, in plain words; `None` when it can.
pub fn refusal(path: &str) -> Option<&'static str> {
    if !segments_ok(path) {
        Some("The path must be a plain relative path with no `..`, `.git` or backslash.")
    } else if is_policy(path) {
        Some("A check cannot add a policy, CI or manifest file. It may only add a test file.")
    } else if !is_test_path(path) {
        Some(
            "A check may only add a test file: put it in a tests, test, __tests__, spec, specs or e2e folder, or name it test_x, x.test.y, x_test.y or x.spec.y.",
        )
    } else {
        None
    }
}

/// One entry of the worker's change: git's mode after the change and its status letter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub status: char,
    pub mode: String,
    pub path: String,
}

/// Reads `git diff --raw -z --no-renames --no-abbrev` output: `:<old mode> <new mode> <old id> <new id> <status>` NUL path NUL.
pub fn parse_raw(out: &[u8]) -> Result<Vec<Change>, String> {
    let text = String::from_utf8_lossy(out);
    let mut tokens = text.split('\0').filter(|t| !t.is_empty());
    let mut changes = Vec::new();
    while let Some(header) = tokens.next() {
        let path = tokens
            .next()
            .ok_or_else(|| "The change could not be read.".to_owned())?;
        let fields: Vec<&str> = header.trim_start_matches(':').split(' ').collect();
        let &[_, mode, _, _, status] = fields.as_slice() else {
            return Err("The change could not be read.".into());
        };
        changes.push(Change {
            status: status.chars().next().unwrap_or('?'),
            mode: mode.to_owned(),
            path: path.to_owned(),
        });
    }
    Ok(changes)
}

/// Judges the worker's whole change against the rule: the paths added, or the plain reason it is refused. `wanted` is the path the
/// person proposed, which must be one of them.
pub fn judge(changes: &[Change], wanted: &str) -> Result<Vec<String>, String> {
    if changes.is_empty() {
        return Err("The worker committed no change, so there is no test file to prove.".into());
    }
    if changes.len() > MOST_FILES {
        return Err(format!(
            "The change adds more than {MOST_FILES} files. A check adds a few test files."
        ));
    }
    for change in changes {
        if change.status != 'A' {
            return Err(format!(
                "The change touches {}, which is not a new file. A check only adds test files.",
                change.path
            ));
        }
        if !matches!(change.mode.as_str(), "100644" | "100755") {
            return Err(format!(
                "{} is a link or a submodule, not a plain test file.",
                change.path
            ));
        }
        if let Some(why) = refusal(&change.path) {
            return Err(format!("{}: {why}", change.path));
        }
    }
    if !changes.iter().any(|c| c.path == wanted) {
        return Err(format!(
            "The change does not add the proposed file {wanted}."
        ));
    }
    Ok(changes.iter().map(|c| c.path.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_files_pass_and_everything_else_is_refused() {
        for path in [
            "tests/escape_check.rs",
            "src/tests/a.txt",
            "web/src/a.test.ts",
            "pkg/test_parse.py",
            "pkg/parse_test.go",
            "e2e/login.spec.ts",
            "Tests/Upper.txt",
        ] {
            assert!(is_test_path(path), "{path}");
            assert_eq!(refusal(path), None, "{path}");
        }
        for path in [
            "src/lib.rs",
            "docs/readme.md",
            ".orbyn/crew.json",
            ".github/workflows/ci.yml",
            "tests/conftest.py",
            "tests/../src/lib.rs",
            "/tests/a.rs",
            "tests\\a.rs",
            "tests/.git/config",
            "tests//a.rs",
            "tests/**",
            "tests/a?.rs",
            "tests/[a].rs",
            "package.json",
            "tests/package.json",
            "tests/Makefile",
            "tests/vitest.config.ts",
            "Cargo.toml",
            "tests",
            "",
        ] {
            assert!(!is_test_path(path), "{path}");
            assert!(refusal(path).is_some(), "{path}");
        }
    }

    #[test]
    fn a_change_must_only_add_plain_test_files_and_include_the_proposed_one() {
        let add = |path: &str| Change {
            status: 'A',
            mode: "100644".into(),
            path: path.into(),
        };
        assert_eq!(
            judge(&[add("tests/a.rs")], "tests/a.rs").unwrap(),
            vec!["tests/a.rs"]
        );
        assert!(judge(&[], "tests/a.rs").is_err());
        assert!(judge(&[add("tests/b.rs")], "tests/a.rs").is_err());
        assert!(judge(&[add("tests/a.rs"), add("src/lib.rs")], "tests/a.rs").is_err());
        assert!(judge(&[add("tests/a.rs"), add(".orbyn/crew.json")], "tests/a.rs").is_err());
        let mut modified = add("tests/a.rs");
        modified.status = 'M';
        assert!(judge(&[modified], "tests/a.rs").is_err());
        let mut link = add("tests/a.rs");
        link.mode = "120000".into();
        assert!(judge(&[link], "tests/a.rs").is_err());
        let many: Vec<Change> = (0..=MOST_FILES)
            .map(|n| add(&format!("tests/{n}.rs")))
            .collect();
        assert!(judge(&many, "tests/0.rs").is_err());
    }

    #[test]
    fn the_raw_diff_is_read_with_modes_and_statuses() {
        let raw =
            b":000000 100644 0000000 abc1234 A\0tests/a.rs\0:100644 100644 abc def M\0src/lib.rs\0";
        let changes = parse_raw(raw).unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(
            (
                changes[0].status,
                changes[0].mode.as_str(),
                changes[0].path.as_str()
            ),
            ('A', "100644", "tests/a.rs")
        );
        assert_eq!(changes[1].status, 'M');
        assert!(parse_raw(b":000000 100644 0 a A\0").is_err());
        assert!(parse_raw(b":000000 100644 0 A\0tests/a.rs\0").is_err());
    }
}
