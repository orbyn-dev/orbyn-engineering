//! What a merge must not carry unseen. The agents are kept from editing `.orbyn/`, the answer key's protected files and the files a
//! check runs by the always-denied list, which is a heuristic over what reaches the gate. This is the backstop at the point the work
//! lands: a diff that touches any of them, or CI config, is held for a person whatever the mode, and a person's release lets it go.
//! A merge that rewrote `.orbyn/crew.json` or the script a check calls would otherwise widen what later tasks may reach, or turn a
//! check into `exit 0`, and stay green.

use orbyn_contracts::CrewPathPattern;

use crate::crew::autopilot::shell::simple_commands;
use crate::crew::autopilot::tier::is_ci;
use crate::crew::checks::covers;
use crate::governance::policy::glob_match;

/// The words of the check commands that can name a file or folder, as repository-relative words (no `./`, no variable, not absolute).
pub fn check_words(commands: &[String]) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for command in commands.iter().flat_map(|c| simple_commands(c)) {
        for word in std::iter::once(&command.raw)
            .chain(&command.args)
            .chain(&command.redirects)
        {
            let word = word.strip_prefix("./").unwrap_or(word).replace('\\', "/");
            if word.is_empty() || word.starts_with(['-', '/', '$', '~', '%']) || word.contains("..")
            {
                continue;
            }
            if !words.contains(&word) {
                words.push(word);
            }
        }
    }
    words
}

/// Config files each runner reads, by the program that runs it. A match is by file name, or by path for one with a folder.
const RUNNER_CONFIGS: &[(&[&str], &[&str])] = &[
    (
        &["npm", "yarn", "pnpm", "npx", "bun"],
        &[
            "package.json",
            ".npmrc",
            "pnpm-workspace.yaml",
            ".yarnrc.yml",
            "tsconfig.json",
        ],
    ),
    (
        &["eslint"],
        &[
            "eslint.config.*",
            ".eslintrc*",
            "package.json",
            "tsconfig.json",
        ],
    ),
    (&["tsc"], &["tsconfig*.json"]),
    (&["ruff"], &["ruff.toml", ".ruff.toml", "pyproject.toml"]),
    (&["deno"], &["deno.json", "deno.jsonc"]),
    (
        &["golangci-lint"],
        &[".golangci.yml", ".golangci.yaml", "go.mod"],
    ),
    (
        &["make", "gmake"],
        &["Makefile", "makefile", "GNUmakefile", "*.mk"],
    ),
    (
        &["pytest", "tox"],
        &[
            "conftest.py",
            "pytest.ini",
            "setup.cfg",
            "tox.ini",
            "pyproject.toml",
        ],
    ),
    (
        &["cargo", "rustc", "clippy-driver"],
        &[
            "clippy.toml",
            ".clippy.toml",
            "rustfmt.toml",
            ".rustfmt.toml",
            ".cargo/config.toml",
            ".cargo/config",
            "Cargo.toml",
            "build.rs",
            "rust-toolchain.toml",
        ],
    ),
    (&["go"], &["go.mod", "go.work"]),
    (&["jest"], &["jest.config.*", "package.json"]),
    (
        &["vitest"],
        &["vitest.config.*", "vite.config.*", "package.json"],
    ),
    (&["just"], &["justfile", "Justfile", ".justfile"]),
    (&["mvn"], &["pom.xml"]),
    (
        &["gradle", "gradlew"],
        &["build.gradle*", "settings.gradle*"],
    ),
];

/// The config patterns of the runners the checks use. `python -m pytest` counts as pytest.
pub fn runner_configs(commands: &[String]) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    for command in commands.iter().flat_map(|c| simple_commands(c)) {
        let mut names = vec![command.program.clone()];
        if let Some(at) = command.args.iter().position(|a| a == "-m") {
            names.extend(command.args.get(at + 1).cloned());
        }
        for (programs, configs) in RUNNER_CONFIGS {
            if names.iter().any(|n| programs.contains(&n.as_str())) {
                found.extend(configs.iter().copied());
            }
        }
    }
    found
}

fn config_matches(path: &str, pattern: &str) -> bool {
    if pattern.contains('/') {
        return path == pattern || path.ends_with(&format!("/{pattern}"));
    }
    let base = path.rsplit('/').next().unwrap_or(path);
    glob_match(pattern, base)
}

/// Whether a word of a check command covers `path`: the same file, a folder it names (with or without a trailing slash), or a glob
/// matched as a pattern.
fn word_covers(word: &str, path: &str) -> bool {
    if word.contains(['*', '?', '[']) {
        // `*` here matches across folders, so this holds more, never less. `?` and `[..]` are read as `*`.
        let pattern: String = word
            .chars()
            .map(|c| if matches!(c, '?' | '[') { '*' } else { c })
            .collect();
        // No test that the file exists: a new file that matches is one the check's loop will run.
        return glob_match(&pattern, path);
    }
    let folder = word.trim_end_matches('/');
    !folder.is_empty() && (path == folder || path.starts_with(&format!("{folder}/")))
}

/// Why a diff is held, in words; empty when nothing sensitive is touched.
pub fn sensitive(
    paths: &[String],
    protected: &[CrewPathPattern],
    checks: &[String],
) -> Vec<String> {
    let mut why: Vec<String> = Vec::new();
    let mut note = |text: &str| {
        if !why.iter().any(|w| w == text) {
            why.push(text.to_owned());
        }
    };
    let named = check_words(checks);
    let configs = runner_configs(checks);
    for path in paths {
        if path.starts_with(".orbyn/") {
            note("it changes .orbyn/, the crew's own rules");
        }
        if protected.iter().any(|p| covers(p, path)) {
            note("it changes a file the answer key protects");
        }
        if is_ci(path) {
            note("it changes CI config");
        }
        if named.iter().any(|w| word_covers(w, path)) {
            note("it changes a file one of the repository's checks runs or reads");
        }
        if configs.iter().any(|c| config_matches(path, c)) {
            note("it changes the config of a runner the repository's checks use");
        }
    }
    why
}

/// The review lanes a merge is owed: the general code review (`CODE_LANE`) for anything that is not low, and `SECURITY_LANE` for high.
pub fn owed_lanes(risk: super::Risk) -> Vec<String> {
    match risk {
        super::Risk::Low => Vec::new(),
        super::Risk::Medium => vec![crate::crew::CODE_LANE.to_owned()],
        super::Risk::High => vec![
            crate::crew::CODE_LANE.to_owned(),
            crate::crew::SECURITY_LANE.to_owned(),
        ],
    }
}
