//! The risk tier of a diff: low, medium or high (docs/crew.md, "Autopilot"). It follows the paths, the size and the
//! revert rate of the area and the engine over the last 60 days, and it carries its reasons. Pure: the facts come in, a tier
//! goes out. Coverage of the touched code is not measured yet, so it is never a reason, and the score says so.

use std::collections::BTreeSet;

use crate::crew::autopilot::tier::is_ci;

/// How risky a diff is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Risk {
    Low,
    Medium,
    High,
}

impl Risk {
    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// Merges that touched an area (or were made by an engine) in the window, and how many of them were reverted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rate {
    pub merges: u32,
    pub reverts: u32,
}

/// A diff of more than this many files or lines is large.
pub const LARGE_FILES: u32 = 20;
pub const LARGE_LINES: u32 = 400;
/// The revert rate, in permille, at which an area or an engine is risky, and the merges it needs before it counts.
pub const HIGH_REVERT_PERMILLE: u32 = 200;
pub const MIN_SAMPLE: u32 = 10;
/// The window of the revert rate.
pub const WINDOW_DAYS: i64 = 60;

/// What the score reads.
#[derive(Clone, Debug, Default)]
pub struct DiffFacts {
    pub paths: Vec<String>,
    pub files: u32,
    pub lines: u32,
    /// The revert rate of each area the diff touches.
    pub areas: Vec<(String, Rate)>,
    pub engine: Option<(String, Rate)>,
}

/// A tier and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scored {
    pub risk: Risk,
    pub reasons: Vec<String>,
    /// What the score did not use, so a tier never implies more than it read.
    pub not_measured: Vec<&'static str>,
}

/// The area of a path: its first two folders, or the first one, or the file itself at the root.
pub fn area_of(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    match parts.len() {
        0 => String::new(),
        1 => parts[0].to_owned(),
        _ => format!("{}/{}", parts[0], parts[1]),
    }
}

/// The areas of a set of paths, once each.
pub fn areas_of(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|p| area_of(p))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

const SENSITIVE_WORDS: [&str; 16] = [
    "auth",
    "login",
    "oauth",
    "session",
    "password",
    "credential",
    "secret",
    "keychain",
    "crypto",
    "cipher",
    "tls",
    "jwt",
    "token",
    "permission",
    "keystore",
    "signing",
];
const DEPENDENCY_FILES: [&str; 16] = [
    "cargo.toml",
    "cargo.lock",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lock",
    "go.mod",
    "go.sum",
    "pyproject.toml",
    "poetry.lock",
    "uv.lock",
    "gemfile",
    "gemfile.lock",
    "composer.json",
    "composer.lock",
];

fn words(path: &str) -> Vec<String> {
    path.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn is_migration(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower
        .split('/')
        .any(|part| part == "migrations" || part == "migration" || part.starts_with("migrate"))
        || lower.contains("/migrations.rs")
}

fn is_docs(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.starts_with("docs/") || lower.ends_with(".md") || lower.ends_with(".mdx")
}

fn is_test(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower
        .split('/')
        .any(|part| matches!(part, "tests" | "test" | "__tests__" | "e2e" | "fixtures"))
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.rsplit('/').next().is_some_and(|f| {
            f.ends_with("_test.rs") || f.ends_with("_test.go") || f.starts_with("test_")
        })
}

/// The tier of a diff, with the reasons that set it.
pub fn score(facts: &DiffFacts) -> Scored {
    let mut high: Vec<String> = Vec::new();
    let mut note = |why: String| {
        if !high.contains(&why) {
            high.push(why);
        }
    };
    for path in &facts.paths {
        let lower = path.to_lowercase();
        let base = lower.rsplit('/').next().unwrap_or(&lower).to_owned();
        if is_ci(path) {
            note("it changes CI".to_owned());
        }
        if is_migration(path) {
            note("it changes a migration".to_owned());
        }
        if DEPENDENCY_FILES.contains(&base.as_str()) || lower.ends_with(".cargo/config.toml") {
            note("it changes dependencies".to_owned());
        }
        // Words of the path, not letters: `author` is not `auth`.
        if !is_docs(path)
            && !is_test(path)
            && words(path)
                .iter()
                .any(|w| SENSITIVE_WORDS.contains(&w.as_str()))
        {
            note("it touches auth, keys or crypto".to_owned());
        }
    }
    if facts.files > LARGE_FILES || facts.lines > LARGE_LINES {
        note(format!(
            "the diff is large ({} files, {} lines)",
            facts.files, facts.lines
        ));
    }
    let hot = |rate: &Rate| {
        rate.merges >= MIN_SAMPLE && rate.reverts * 1000 >= HIGH_REVERT_PERMILLE * rate.merges
    };
    for (area, r) in &facts.areas {
        if hot(r) {
            note(format!(
                "{area} was reverted in {} of its last {} merges",
                r.reverts, r.merges
            ));
        }
    }
    if let Some((engine, r)) = &facts.engine
        && hot(r)
    {
        note(format!(
            "{engine}'s merges were reverted {} times in its last {}",
            r.reverts, r.merges
        ));
    }
    let not_measured = vec!["test coverage of the touched code"];
    if !high.is_empty() {
        return Scored {
            risk: Risk::High,
            reasons: high,
            not_measured,
        };
    }
    let all_low = !facts.paths.is_empty() && facts.paths.iter().all(|p| is_docs(p) || is_test(p));
    if all_low && facts.files <= 5 && facts.lines <= 100 {
        return Scored {
            risk: Risk::Low,
            reasons: vec!["only docs or tests, and small".to_owned()],
            not_measured,
        };
    }
    Scored {
        risk: Risk::Medium,
        reasons: vec!["ordinary feature work".to_owned()],
        not_measured,
    }
}
