//! `classes` in `.orbyn/crew.json`, read at a commit and parsed strictly. Orbyn only reads this file: it is never written, and
//! the checkout's copy is never the rule, because a worker can edit it. Where the older readers of the file (`review::rules`,
//! `autopilot::hosts`) fail open or check late, this one does not: the `.orbyn` entry must be a tree (a symlink there would read
//! as a missing file), the file must be a plain blob (mode 100644: not a symlink, not a submodule), and its size is read before
//! its content. A file that cannot be read or does not parse blocks every engine; it is never read as "no rules".

use std::path::Path;

use orbyn_contracts::CrewToken;
use serde::Deserialize;
use serde_json::Value;

use super::glob::Glob;
use crate::crew::{RULES_FILE, find_secret, redact};
use crate::source_control::git::{git_hardened as git, text};

/// Larger than any sane file.
pub const FILE_MAX: u64 = 64 * 1024;
pub const RULES_MAX: usize = 64;
pub const GLOBS_MAX: usize = 32;
pub const ENGINES_MAX: usize = 16;
pub const NAME_MAX: usize = 40;
pub const REASON_MAX: usize = 200;
/// The class of every path no rule covers.
pub const BUILT_IN: &str = "Everything else";
/// The name of the rule that blocks every engine when the file cannot be trusted.
pub const INVALID_RULE: &str = "crew.json invalid";

/// Keys other slices read. Anything else is reported and ignored.
const KNOWN_KEYS: [&str; 5] = ["required", "lanes", "checks", "allowed_hosts", "classes"];

/// One rule: which paths, which engines may take them and why.
#[derive(Clone, Debug)]
pub struct Rule {
    pub name: String,
    /// The patterns as written.
    pub paths: Vec<String>,
    pub(super) globs: Vec<Glob>,
    /// Engine ids. Empty blocks every engine. An id this machine does not have is kept.
    pub engines: Vec<String>,
    pub reason: String,
}

/// What the file says at one commit.
#[derive(Clone, Debug, Default)]
pub struct ClassFile {
    /// The commit read. `None` when no commit could be resolved.
    pub commit: Option<String>,
    /// The file's blob id. `None` when the file is missing, or could not be told apart from a missing one.
    pub blob: Option<String>,
    /// Empty when there is no file, no `classes`, or the file is invalid.
    pub rules: Vec<Rule>,
    /// Why the file cannot be trusted. Any problem blocks every engine.
    pub problems: Vec<String>,
    pub notes: Vec<String>,
}

impl ClassFile {
    pub fn invalid(&self) -> bool {
        !self.problems.is_empty()
    }

    /// The words that block every engine while the file is invalid.
    pub fn problem_text(&self) -> String {
        self.problems.first().cloned().unwrap_or_default()
    }

    fn broken(commit: Option<&str>, blob: Option<String>, problem: String) -> Self {
        Self {
            commit: commit.map(str::to_owned),
            blob,
            problems: vec![redact(&problem)],
            ..Self::default()
        }
    }
}

/// The commit the lead's checkout is at: the base a task starts from. `None` when git cannot say.
pub async fn base_commit(root: &Path) -> Option<String> {
    let out = crate::source_control::git::git(root, &["rev-parse", "HEAD"])
        .await
        .ok()?;
    let sha = text(&out);
    is_object_id(&sha).then_some(sha)
}

/// A ref the rules are also read at: the project's default branch, which a checkout or a commit on the way does not move.
#[derive(Clone, Debug)]
pub struct PinnedRef {
    /// The ref in short words, such as `origin/main`.
    pub name: String,
    /// The commit it pointed at when it was read.
    pub commit: String,
}

/// The default branch tip as of the last fetch, or `None` when the repository has no default-branch ref to read. In order: the
/// remote-tracking ref `origin/HEAD` points at (set by `git clone` or `git remote set-head`); `origin/main`, `origin/master`
/// or `origin/trunk`; then a local branch named by `init.defaultBranch`, `main`, `master` or `trunk`. A remote-tracking ref is
/// moved by `git fetch` and `git pull`, not by `checkout`, `switch`, `reset` or a commit, in any worktree. A local branch is
/// moved by a commit or `git branch -f` on it, so it is the weaker fallback, used only when there is no remote-tracking ref.
/// Neither is out of reach of an agent that runs `git update-ref` in the repository, which the hard rule N7 holds.
pub async fn pinned_ref(root: &Path) -> Option<PinnedRef> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(out) = git(root, &["symbolic-ref", "-q", "refs/remotes/origin/HEAD"]).await {
        let target = text(&out);
        if target.starts_with("refs/remotes/origin/") && target != "refs/remotes/origin/HEAD" {
            candidates.push(target);
        }
    }
    candidates.extend(
        ["main", "master", "trunk"]
            .iter()
            .map(|b| format!("refs/remotes/origin/{b}")),
    );
    if let Ok(out) = git(root, &["config", "--get", "init.defaultBranch"]).await {
        let name = text(&out);
        if !name.is_empty() && !name.starts_with('-') {
            candidates.push(format!("refs/heads/{name}"));
        }
    }
    candidates.extend(
        ["main", "master", "trunk"]
            .iter()
            .map(|b| format!("refs/heads/{b}")),
    );
    for full in candidates {
        let spec = format!("{full}^{{commit}}");
        let Ok(out) = git(root, &["rev-parse", "--verify", "-q", &spec]).await else {
            continue;
        };
        let commit = text(&out);
        if is_object_id(&commit) {
            let name = full
                .strip_prefix("refs/remotes/")
                .or_else(|| full.strip_prefix("refs/heads/"))
                .unwrap_or(&full)
                .to_owned();
            return Some(PinnedRef { name, commit });
        }
    }
    None
}

fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// One entry of `git ls-tree -z`.
struct Entry {
    mode: String,
    kind: String,
    sha: String,
    path: String,
}

fn entries(raw: &[u8]) -> Option<Vec<Entry>> {
    raw.split(|b| *b == 0)
        .filter(|part| !part.is_empty())
        .map(|part| {
            let line = std::str::from_utf8(part).ok()?;
            let (meta, path) = line.split_once('\t')?;
            let mut meta = meta.split(' ');
            Some(Entry {
                mode: meta.next()?.to_owned(),
                kind: meta.next()?.to_owned(),
                sha: meta.next()?.to_owned(),
                path: path.to_owned(),
            })
        })
        .collect()
}

/// Reads and parses the file at `base`. Anything but a plain, small, valid file is a problem, except that no `.orbyn` and no
/// `crew.json` in it is no file.
pub async fn read_class_file(root: &Path, base: Option<&str>) -> ClassFile {
    let Some(base) = base.filter(|b| is_object_id(b)) else {
        return ClassFile::broken(
            None,
            None,
            format!("{RULES_FILE} cannot be read: no commit to read it at"),
        );
    };
    let words = |what: &str| format!("{RULES_FILE} {what}");
    let fail = |blob: Option<String>, what: String| ClassFile::broken(Some(base), blob, what);

    // `.orbyn` must be a tree or absent. As a symlink in the tree, git would list nothing under it and the file would read as missing.
    let dir = match list(root, base, ".orbyn").await {
        Ok(out) => match entries(&out) {
            Some(found) => found,
            None => {
                return fail(
                    None,
                    words("cannot be read: git listed a name that is not text"),
                );
            }
        },
        Err(error) => {
            return fail(
                None,
                words(&format!(
                    "cannot be read at the base commit: {}",
                    error.public_message()
                )),
            );
        }
    };
    match dir.iter().find(|e| e.path == ".orbyn") {
        None => {
            return ClassFile {
                commit: Some(base.to_owned()),
                ..ClassFile::default()
            };
        }
        Some(entry) if entry.mode == "040000" && entry.kind == "tree" => {}
        Some(entry) => {
            return fail(
                None,
                format!(
                    ".orbyn is a {} in the repository, not a folder",
                    kind_words(&entry.mode)
                ),
            );
        }
    }
    let file = match list(root, base, RULES_FILE).await {
        Ok(out) => match entries(&out) {
            Some(found) => found,
            None => {
                return fail(
                    None,
                    words("cannot be read: git listed a name that is not text"),
                );
            }
        },
        Err(error) => {
            return fail(
                None,
                words(&format!(
                    "cannot be read at the base commit: {}",
                    error.public_message()
                )),
            );
        }
    };
    let Some(entry) = file.into_iter().find(|e| e.path == RULES_FILE) else {
        return ClassFile {
            commit: Some(base.to_owned()),
            ..ClassFile::default()
        };
    };
    if entry.mode != "100644" || entry.kind != "blob" || !is_object_id(&entry.sha) {
        return fail(
            None,
            format!(
                "{RULES_FILE} is a {} in the repository, not a plain file",
                kind_words(&entry.mode)
            ),
        );
    }
    let blob = entry.sha;
    let size = match git(root, &["cat-file", "-s", &blob]).await {
        Ok(out) => text(&out).parse::<u64>().ok(),
        Err(_) => None,
    };
    match size {
        None => return fail(Some(blob), words("cannot be read: its size is unknown")),
        Some(size) if size > FILE_MAX => {
            return fail(Some(blob), words("is over 64 KiB, so it is not read"));
        }
        Some(_) => {}
    }
    let bytes = match git(root, &["cat-file", "blob", &blob]).await {
        Ok(bytes) if bytes.len() as u64 <= FILE_MAX => bytes,
        Ok(_) => return fail(Some(blob), words("is over 64 KiB, so it is not read")),
        Err(error) => {
            return fail(
                Some(blob),
                words(&format!("cannot be read: {}", error.public_message())),
            );
        }
    };
    let Ok(source) = String::from_utf8(bytes) else {
        return fail(Some(blob), words("is not UTF-8 text"));
    };
    match parse(&source) {
        Ok((rules, notes)) => ClassFile {
            commit: Some(base.to_owned()),
            blob: Some(blob),
            rules,
            problems: Vec::new(),
            notes,
        },
        Err(problem) => fail(Some(blob), problem),
    }
}

/// `git ls-tree -z` of one path at `base`, matched literally.
async fn list(root: &Path, base: &str, path: &str) -> crate::error::Result<Vec<u8>> {
    git(
        root,
        &["--literal-pathspecs", "ls-tree", "-z", base, "--", path],
    )
    .await
}

fn kind_words(mode: &str) -> &'static str {
    match mode {
        "120000" => "symlink",
        "160000" => "submodule",
        "040000" => "folder",
        _ => "file with an unexpected mode",
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Section {
    version: u32,
    rules: Vec<Draft>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    name: String,
    paths: Vec<String>,
    engines: Vec<String>,
    reason: String,
}

/// Cuts text for a message, and redacts it: a parse error can quote what it read.
fn quoted(text: &str) -> String {
    let cut: String = redact(text).chars().take(80).collect();
    cut.replace('`', "'")
}

/// The rules of a file's text and the notes about it, or the first problem in plain words.
pub(in crate::crew) fn parse(source: &str) -> Result<(Vec<Rule>, Vec<String>), String> {
    let value: Value = serde_json::from_str(source).map_err(|error| {
        format!(
            "{RULES_FILE} is not valid JSON (line {}, column {})",
            error.line(),
            error.column()
        )
    })?;
    if let Some(class) = find_secret(&value) {
        return Err(format!(
            "{RULES_FILE} holds a {class}, so it is refused; it must hold no secret"
        ));
    }
    let Value::Object(top) = value else {
        return Err(format!("{RULES_FILE} must be a JSON object"));
    };
    let mut notes: Vec<String> = top
        .keys()
        .filter(|key| !KNOWN_KEYS.contains(&key.as_str()))
        .map(|key| {
            format!(
                "`{}` is not read by this version and is ignored",
                quoted(key)
            )
        })
        .collect();
    notes.sort();
    let Some(section) = top.get("classes") else {
        return Ok((Vec::new(), notes));
    };
    let section: Section = serde_json::from_value(section.clone())
        .map_err(|error| format!("classes: {}", quoted(&error.to_string())))?;
    if section.version != 1 {
        return Err(format!(
            "classes: version {} is not supported; this version reads 1",
            section.version
        ));
    }
    if section.rules.len() > RULES_MAX {
        return Err(format!(
            "classes: {} rules; the most is {RULES_MAX}",
            section.rules.len()
        ));
    }
    let mut rules: Vec<Rule> = Vec::new();
    for draft in section.rules {
        let rule = one_rule(draft, &rules)?;
        rules.push(rule);
    }
    Ok((rules, notes))
}

fn one_rule(draft: Draft, before: &[Rule]) -> Result<Rule, String> {
    let name = draft.name;
    let fine = !name.is_empty()
        && name.chars().count() <= NAME_MAX
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !fine {
        return Err(format!(
            "classes: the name `{}` must be 1 to {NAME_MAX} characters of a-z, 0-9 and -",
            quoted(&name)
        ));
    }
    if before.iter().any(|r| r.name == name) {
        return Err(format!("classes: the name `{name}` is used twice"));
    }
    let in_rule = |what: String| format!("classes: rule `{name}`: {what}");
    let reason = draft.reason;
    if reason.trim().is_empty()
        || reason.chars().count() > REASON_MAX
        || reason.chars().any(char::is_control)
    {
        return Err(in_rule(format!(
            "the reason must be 1 to {REASON_MAX} characters on one line"
        )));
    }
    if draft.paths.is_empty() || draft.paths.len() > GLOBS_MAX {
        return Err(in_rule(format!(
            "it needs 1 to {GLOBS_MAX} paths, not {}",
            draft.paths.len()
        )));
    }
    if draft.engines.len() > ENGINES_MAX {
        return Err(in_rule(format!(
            "it names {} engines; the most is {ENGINES_MAX}",
            draft.engines.len()
        )));
    }
    let mut globs = Vec::new();
    for path in &draft.paths {
        globs.push(
            Glob::parse(path)
                .map_err(|why| in_rule(format!("the path `{}` {why}", quoted(path))))?,
        );
    }
    let mut engines: Vec<String> = Vec::new();
    for engine in &draft.engines {
        CrewToken::new(engine.clone()).map_err(|_| {
            in_rule(format!(
                "`{}` is not an engine id (lowercase letters, digits, - and _)",
                quoted(engine)
            ))
        })?;
        if !engines.contains(engine) {
            engines.push(engine.clone());
        }
    }
    Ok(Rule {
        name,
        paths: draft.paths,
        globs,
        engines,
        reason,
    })
}
