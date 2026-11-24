//! Which path a created check may add (docs/crew/escapes.md, decision 4: "a created check only adds test files, never policy files").
//! The design names no globs, so these are the ones this version uses, and they are the only rule: the core compares the diff of the
//! worker's change against them and refuses anything else, whatever the worker or its brief said.
//!
//! A path is a **test file** when it is relative and plain ASCII; has no empty, `.`, `..` or hidden (leading `.`) segment, no segment
//! ending in a dot or a space, no Windows reserved name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, with or without an
//! extension), no backslash, colon, quote, control character or glob character (`* ? [ ] { } !`) and at most 200 bytes; is not a **policy
//! file** (below); and either a folder in it is `tests`, `test`, `__tests__`, `spec`, `specs` or `e2e`, or its name starts `test_` or holds
//! `.test.`, `.spec.`, `_test.` or `_spec.`. Case is ignored. Non-ASCII is refused rather than normalised, so no look-alike spelling can
//! name another file.
//!
//! A **policy file** is never a test file, even inside a test folder: anything under `.github/`, `.orbyn/`, `.circleci/`, `.gitlab*` or
//! `.cargo/` (and every hidden segment, which also covers `.gitattributes`, `.gitignore`, `.gitmodules`, `.babelrc` and `.env`), and the
//! manifests, lock files, build files and runner or tool configuration that decide what a check command does: `package.json`,
//! `Makefile`, `justfile`, `Taskfile.yml`, `tox.ini`, `noxfile.py`, `pyproject.toml`, `build.rs`, `conftest.py`, `pytest.ini`,
//! `setup.cfg`, `setup.py`, `Cargo.toml`, `Cargo.lock`, `go.mod`, `go.sum`, `CMakeLists.txt`, `pom.xml`, `Dockerfile`, `BUILD`,
//! `BUILD.bazel`, `WORKSPACE`, `rust-toolchain*`, `build.gradle*`, `settings.gradle*`, `tsconfig*.json`, `babel.config.*`,
//! `jest.config.*`, `vitest.config.*`, `playwright.config.*`, `karma.conf.*`, `docker-compose*`, and the lock files of npm, yarn and pnpm.
//! `conftest.py` is a policy file: it changes how every test in its folder runs, so a check cannot add one.
//!
//! The worker's change is **exactly one added file**, the path the person proposed: the key's `file_scope` allows that one path, so a
//! second file would already fail the worker's verification, and the core judges it again from git. A modified, deleted, renamed, copied
//! or type-changed path, a symlink or a submodule is refused.

/// The longest path, in bytes.
pub const PATH_MAX: usize = 200;

const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];

fn reserved(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment).to_lowercase();
    RESERVED.contains(&stem.as_str())
        || (stem.len() == 4
            && (stem.starts_with("com") || stem.starts_with("lpt"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn segments_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= PATH_MAX
        && path.is_ascii()
        && !path.starts_with('/')
        && !path.contains([
            '\\', ':', '*', '?', '[', ']', '{', '}', '!', '"', '\'', '$', '`', ';', '&', '|', '<',
            '>',
        ])
        && !path.chars().any(char::is_control)
        && path.split('/').all(|s| {
            !s.is_empty()
                && !s.starts_with('.')
                && !s.ends_with('.')
                && !s.ends_with(' ')
                && !reserved(s)
                && !short_name(s)
        })
}

/// A Windows 8.3 short name (`PROGRA~1`, `x~2.txt`): another spelling of a longer name the rules above would have judged.
fn short_name(segment: &str) -> bool {
    segment
        .match_indices('~')
        .any(|(at, _)| segment[at + 1..].starts_with(|c: char| c.is_ascii_digit()))
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
                | "package-lock.json"
                | "yarn.lock"
                | "pnpm-lock.yaml"
                | "go.mod"
                | "go.sum"
                | "cmakelists.txt"
                | "pom.xml"
                | "dockerfile"
                | "build"
                | "build.bazel"
                | "workspace"
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
                | "gemfile"
                | "gemfile.lock"
                | "rakefile"
                | "global.json"
                | "deno.json"
                | "deno.jsonc"
                | "deno.lock"
                | "bunfig.toml"
                | "meson.build"
                | "buck"
                | "sitecustomize.py"
                | "usercustomize.py"
                | "directory.build.props"
                | "directory.build.targets"
                | "directory.packages.props"
        )
        || [
            "jest.config.",
            "vitest.config.",
            "playwright.config.",
            "babel.config.",
            "karma.conf.",
            "rust-toolchain",
            "build.gradle",
            "settings.gradle",
            "docker-compose",
            "vite.config.",
            "vitest.workspace.",
            "cypress.config.",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || (name.starts_with("tsconfig") && name.ends_with(".json"))
        || (name.starts_with("requirements") && name.ends_with(".txt"))
        || [
            ".csproj", ".fsproj", ".vbproj", ".sln", ".props", ".targets",
        ]
        .iter()
        .any(|suffix| name.ends_with(suffix))
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

/// Judges the worker's whole change against the rule: the one path added, or the plain reason it is refused. `wanted` is the path the
/// person proposed, which must be the only change.
pub fn judge(changes: &[Change], wanted: &str) -> Result<Vec<String>, String> {
    if changes.is_empty() {
        return Err("The worker committed no change, so there is no test file to prove.".into());
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
        if change.path != wanted {
            return Err(format!(
                "The change adds {}, but a check adds only the proposed file {wanted}.",
                change.path
            ));
        }
    }
    if changes.len() != 1 {
        return Err("The change adds more than the one proposed file.".into());
    }
    Ok(vec![wanted.to_owned()])
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
            "tests/a.rs.",
            "tests/a ",
            "tests/trailing./a.rs",
            "tests/con.rs",
            "tests/CON",
            "tests/aux.test.ts",
            "tests/Nul.txt",
            "tests/com1.rs",
            "tests/LPT9.test.js",
            "tests/caf\u{e9}.test.js",
            "tests/.hidden.test.js",
            "tests/.gitattributes",
            "tests/.gitignore",
            "tests/.gitmodules",
            "tests/.babelrc",
            "tests/tsconfig.json",
            "tests/tsconfig.app.json",
            "tests/playwright.config.ts",
            "tests/jest.config.js",
            "tests/pytest.ini",
            "tests/go.mod",
            "tests/BUILD",
            "tests/BUILD.bazel",
            "tests/CMakeLists.txt",
            "tests/build.gradle",
            "tests/build.gradle.kts",
            "tests/rust-toolchain.toml",
            "tests/Cargo.toml",
            "tests/package-lock.json",
            "tests/Dockerfile",
            "tests/conftest.py",
            "package.json",
            "tests/package.json",
            "tests/Makefile",
            "tests/vitest.config.ts",
            "tests/Gemfile",
            "tests/Rakefile",
            "tests/requirements.txt",
            "tests/requirements-dev.txt",
            "tests/Directory.Build.props",
            "tests/app.csproj",
            "tests/app.fsproj",
            "tests/global.json",
            "tests/deno.json",
            "tests/bunfig.toml",
            "tests/vite.config.ts",
            "tests/vitest.workspace.ts",
            "tests/cypress.config.js",
            "tests/meson.build",
            "tests/BUCK",
            "tests/sitecustomize.py",
            "tests/CONIN$",
            "tests/CONOUT$.txt",
            "tests/PROGRA~1.test.js",
            "tests/a~1.test.js",
            "tests/x~12/a.test.js",
            "Cargo.toml",
            "tests",
            "",
        ] {
            assert!(!is_test_path(path), "{path}");
            assert!(refusal(path).is_some(), "{path}");
        }
    }

    #[test]
    fn a_change_must_be_exactly_the_one_proposed_added_file() {
        let add = |path: &str| Change {
            status: 'A',
            mode: "100644".into(),
            path: path.into(),
        };
        assert_eq!(
            judge(&[add("tests/a.rs")], "tests/a.rs").unwrap(),
            vec!["tests/a.rs"]
        );
        assert_eq!(
            judge(&[], "tests/a.rs").unwrap_err(),
            "The worker committed no change, so there is no test file to prove."
        );
        assert_eq!(
            judge(&[add("tests/b.rs")], "tests/a.rs").unwrap_err(),
            "The change adds tests/b.rs, but a check adds only the proposed file tests/a.rs."
        );
        assert_eq!(
            judge(&[add("tests/a.rs"), add("src/lib.rs")], "tests/a.rs").unwrap_err(),
            format!(
                "src/lib.rs: {}",
                refusal("src/lib.rs").expect("a source file is refused")
            )
        );
        assert_eq!(
            judge(&[add("tests/a.rs"), add("tests/b.rs")], "tests/a.rs").unwrap_err(),
            "The change adds tests/b.rs, but a check adds only the proposed file tests/a.rs."
        );
        assert_eq!(
            judge(&[add("tests/a.rs"), add(".orbyn/crew.json")], "tests/a.rs").unwrap_err(),
            format!(
                ".orbyn/crew.json: {}",
                refusal(".orbyn/crew.json").expect("a policy file is refused")
            )
        );
        let mut modified = add("tests/a.rs");
        modified.status = 'M';
        assert_eq!(
            judge(&[modified], "tests/a.rs").unwrap_err(),
            "The change touches tests/a.rs, which is not a new file. A check only adds test files."
        );
        let mut link = add("tests/a.rs");
        link.mode = "120000".into();
        assert_eq!(
            judge(&[link], "tests/a.rs").unwrap_err(),
            "tests/a.rs is a link or a submodule, not a plain test file."
        );
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
        assert_eq!(
            parse_raw(b":000000 100644 0 a A\0").unwrap_err(),
            "The change could not be read."
        );
        assert_eq!(
            parse_raw(b":000000 100644 0 A\0tests/a.rs\0").unwrap_err(),
            "The change could not be read."
        );
    }
}
