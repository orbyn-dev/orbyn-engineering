//! Which classes a brief's files are in. Every pattern of `brief.files` counts, read and write alike. A brief with no files is
//! unknown, and unknown is every class. A file lease (`dir/**`) is checked two ways: the glob is walked against the directory
//! (a worker with the lease may create a matching path), and the tracked files under it, listed from git at the same commit, are
//! matched one by one. A tracked symlink under a lease is followed inside the tree, and what it reaches is classified as if it
//! were leased too; a target that is outside the tree, absolute or missing counts as every class, as does a listing of more than
//! 50,000 files. Writes through a symlink the worker adds later, and reads a shell makes outside the lease, are not seen here.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use orbyn_contracts::CrewToken;

use super::file::{BUILT_IN, ClassFile, INVALID_RULE, Rule};
use super::glob::{fold, path_problem};
use crate::source_control::git::{git_hardened as git, text};

/// Listed tracked files under one lease, at the most.
pub const TRACKED_MAX: usize = 50_000;
/// Paths examined for one brief, in all its leases, at the most. More counts as unknown: every class.
pub const BRIEF_PATHS_MAX: usize = 100_000;
/// Glob tests for one brief, charged at every pattern for every path. More counts as unknown: every class.
pub const BRIEF_TESTS_MAX: usize = 2_000_000;
/// Paths shown per class.
pub const SAMPLES_MAX: usize = 20;
/// Symlinks followed for one brief, at the most.
const LINKS_MAX: usize = 64;
/// A symlink's target is a short path.
const TARGET_MAX: usize = 4096;

/// A class a brief's files are in.
#[derive(Clone, Debug)]
pub struct Hit {
    /// Index into the file's rules.
    pub rule: usize,
    /// The rule's pattern that matched, or a stand-in when every class counted.
    pub pattern: String,
    pub samples: Vec<String>,
}

/// An engine a class does not allow, and the rule's words.
#[derive(Clone, Debug)]
pub struct Block {
    pub rule: String,
    pub reason: String,
    /// The engines the rule allows, as ids.
    pub allowed: Vec<String>,
}

/// The file and what the brief's files matched in it.
#[derive(Clone, Debug, Default)]
pub struct PathClasses {
    pub file: ClassFile,
    pub hits: Vec<Hit>,
    /// Why every class counted, when it did.
    pub unknown: Option<String>,
}

impl PathClasses {
    /// The file and nothing matched: an invalid file or a repository with no rules.
    pub fn of_file(file: ClassFile) -> Self {
        Self {
            file,
            ..Self::default()
        }
    }

    /// Whether the classification says anything: a file was read, or a class matched.
    pub fn applies(&self) -> bool {
        self.file.invalid() || !self.hits.is_empty() || self.file.blob.is_some()
    }

    pub fn rule(&self, hit: &Hit) -> &Rule {
        &self.file.rules[hit.rule]
    }

    /// The first rule that does not allow `engine`, or the invalid file's rule.
    pub fn block_of(&self, engine: &CrewToken) -> Option<Block> {
        if self.file.invalid() {
            return Some(Block {
                rule: INVALID_RULE.to_owned(),
                reason: self.file.problem_text(),
                allowed: Vec::new(),
            });
        }
        self.hits.iter().find_map(|hit| {
            let rule = self.rule(hit);
            (!rule.engines.iter().any(|e| e == engine.as_str())).then(|| Block {
                rule: rule.name.clone(),
                reason: rule.reason.clone(),
                allowed: rule.engines.clone(),
            })
        })
    }

    /// The engines every matching class allows, as ids. `None` when no class matched: any engine. An invalid file allows none.
    pub fn allowed(&self) -> Option<Vec<String>> {
        if self.file.invalid() {
            return Some(Vec::new());
        }
        let mut now: Option<Vec<String>> = None;
        for hit in &self.hits {
            let rule = self.rule(hit);
            now = Some(match now {
                None => rule.engines.clone(),
                Some(have) => have
                    .into_iter()
                    .filter(|e| rule.engines.contains(e))
                    .collect(),
            });
        }
        now
    }

    /// The names of the classes that applied, in the file's order.
    pub fn names(&self) -> Vec<String> {
        if self.file.invalid() {
            return vec![INVALID_RULE.to_owned()];
        }
        if self.hits.is_empty() {
            return vec![BUILT_IN.to_owned()];
        }
        self.hits
            .iter()
            .map(|hit| self.rule(hit).name.clone())
            .collect()
    }
}

/// Something to classify: a path, or a whole lease.
#[derive(Clone, Debug)]
struct Probe {
    path: String,
    tree: bool,
    /// Reached through a symlink, so what it names has to exist.
    via_link: bool,
}

/// How much work one classification may do before it gives up and says unknown.
#[derive(Clone, Copy)]
struct Limits {
    /// Tracked files listed under one lease.
    tracked: usize,
    /// Paths examined in all.
    paths: usize,
    /// Glob tests in all.
    tests: usize,
}

const BRIEF: Limits = Limits {
    tracked: TRACKED_MAX,
    paths: BRIEF_PATHS_MAX,
    tests: BRIEF_TESTS_MAX,
};

/// The tester checks a person's few paths, so it does a tenth of the work.
const TESTER: Limits = Limits {
    tracked: TRACKED_MAX / 10,
    paths: BRIEF_PATHS_MAX / 10,
    tests: BRIEF_TESTS_MAX / 10,
};

struct Walk<'a> {
    root: &'a Path,
    base: &'a str,
    file: Arc<ClassFile>,
    limits: Limits,
    /// Every pattern of every rule: what one path costs.
    globs: usize,
    paths_done: usize,
    tests_done: usize,
    hits: Vec<Hit>,
    unknown: Option<String>,
    seen: BTreeSet<String>,
    links: usize,
}

/// What one batch of paths matched in one rule.
struct Matched {
    rule: usize,
    pattern: String,
    samples: Vec<String>,
}

/// Matches `paths` (and, for a lease, its directory) against every rule. Each path is folded once and the folded form is
/// reused for every pattern. This is the CPU work of a classification, so it runs off the async runtime.
fn match_batch(file: &ClassFile, dir: Option<&str>, paths: &[String]) -> Vec<Matched> {
    let mut found: Vec<Matched> = Vec::new();
    let mut add = |rule: usize, pattern: &str, sample: &str| {
        let at = match found.iter().position(|m| m.rule == rule) {
            Some(at) => at,
            None => {
                found.push(Matched {
                    rule,
                    pattern: pattern.to_owned(),
                    samples: Vec::new(),
                });
                found.len() - 1
            }
        };
        let samples = &mut found[at].samples;
        if samples.len() < SAMPLES_MAX && !samples.iter().any(|s| s == sample) {
            samples.push(sample.to_owned());
        }
    };
    if let Some(dir) = dir {
        let folded = fold(dir);
        for (at, rule) in file.rules.iter().enumerate() {
            if let Some(glob) = rule
                .globs
                .iter()
                .find(|g| g.may_match_under_folded(&folded))
            {
                add(at, glob.text(), &format!("{dir}/**"));
            }
        }
    }
    for path in paths {
        let folded = fold(path);
        for (at, rule) in file.rules.iter().enumerate() {
            if let Some(glob) = rule.globs.iter().find(|g| g.matches_folded(&folded)) {
                add(at, glob.text(), path);
            }
        }
    }
    found
}

/// Classifies the patterns of a brief's `files` (each `path` or `dir/**`) against `file`, using git at `base` for the lease
/// listing and symlinks. With no patterns the brief is unknown.
pub async fn classify(
    root: &Path,
    base: Option<&str>,
    file: ClassFile,
    patterns: &[String],
) -> PathClasses {
    if file.invalid() || file.rules.is_empty() {
        return PathClasses::of_file(file);
    }
    let mut probes = Vec::new();
    for pattern in patterns {
        probes.push(match pattern.strip_suffix("/**") {
            Some(dir) => Probe {
                path: dir.to_owned(),
                tree: true,
                via_link: false,
            },
            None => Probe {
                path: pattern.clone(),
                tree: false,
                via_link: false,
            },
        });
    }
    classify_probes(root, base, file, probes, patterns.is_empty(), BRIEF).await
}

/// Classifies exact paths, such as the ones a person types into the tester.
pub async fn classify_paths(
    root: &Path,
    base: Option<&str>,
    file: ClassFile,
    paths: &[String],
) -> PathClasses {
    if file.invalid() || file.rules.is_empty() {
        return PathClasses::of_file(file);
    }
    let probes = paths
        .iter()
        .map(|path| Probe {
            path: path.clone(),
            tree: false,
            via_link: false,
        })
        .collect();
    classify_probes(root, base, file, probes, paths.is_empty(), TESTER).await
}

async fn classify_probes(
    root: &Path,
    base: Option<&str>,
    file: ClassFile,
    probes: Vec<Probe>,
    no_files: bool,
    limits: Limits,
) -> PathClasses {
    let Some(base) = base.map(str::to_owned) else {
        let why = "no commit to list the files at".to_owned();
        return every_class(file, why);
    };
    if no_files {
        return every_class(file, "the brief names no files".to_owned());
    }
    let file = Arc::new(file);
    let mut walk = Walk {
        root,
        base: &base,
        file: file.clone(),
        limits,
        globs: file.rules.iter().map(|r| r.globs.len()).sum(),
        paths_done: 0,
        tests_done: 0,
        hits: Vec::new(),
        unknown: None,
        seen: BTreeSet::new(),
        links: 0,
    };
    let mut queue = probes;
    while let Some(probe) = queue.pop() {
        if walk.unknown.is_some() {
            break;
        }
        walk.probe(&probe, &mut queue).await;
    }
    let hits = std::mem::take(&mut walk.hits);
    let unknown = walk.unknown.take();
    drop(walk);
    let file = Arc::try_unwrap(file).unwrap_or_else(|shared| (*shared).clone());
    match unknown {
        Some(why) => every_class(file, why),
        None => {
            let mut hits = hits;
            hits.sort_by_key(|hit| hit.rule);
            PathClasses {
                file,
                hits,
                unknown: None,
            }
        }
    }
}

fn every_class(file: ClassFile, why: String) -> PathClasses {
    let hits = file
        .rules
        .iter()
        .enumerate()
        .map(|(rule, _)| Hit {
            rule,
            pattern: "any path".to_owned(),
            samples: Vec::new(),
        })
        .collect();
    PathClasses {
        file,
        hits,
        unknown: Some(why),
    }
}

impl Walk<'_> {
    fn add(&mut self, rule: usize, pattern: &str, sample: &str) {
        let hit = match self.hits.iter().position(|h| h.rule == rule) {
            Some(at) => &mut self.hits[at],
            None => {
                self.hits.push(Hit {
                    rule,
                    pattern: pattern.to_owned(),
                    samples: Vec::new(),
                });
                self.hits.last_mut().expect("just pushed")
            }
        };
        if hit.samples.len() < SAMPLES_MAX && !hit.samples.iter().any(|s| s == sample) {
            hit.samples.push(sample.to_owned());
        }
    }

    async fn probe(&mut self, probe: &Probe, queue: &mut Vec<Probe>) {
        // Keyed on the literal path: two spellings are two listings, and a spelling that lists nothing says so below.
        let key = format!("{}|{}", probe.tree, probe.path);
        if !self.seen.insert(key) {
            return;
        }
        if let Some(why) = path_problem(&probe.path) {
            self.unknown = Some(format!("the path `{}` {why}", probe.path));
            return;
        }
        // A symlink on the way (or the path itself) leads somewhere else: what it reaches is in the lease too.
        let mut ancestors: Vec<String> = Vec::new();
        let mut so_far = String::new();
        for part in probe.path.split('/') {
            if !so_far.is_empty() {
                so_far.push('/');
            }
            so_far.push_str(part);
            ancestors.push(so_far.clone());
        }
        let mut found_any = false;
        let mut links: Vec<(String, String)> = Vec::new();
        match self.list(&ancestors, false, usize::MAX).await {
            Some(listed) => {
                for entry in listed {
                    found_any |= entry.path == probe.path;
                    if entry.mode == "120000" {
                        links.push((entry.path, entry.sha));
                    }
                }
            }
            None => return,
        }
        if probe.via_link && !found_any {
            self.unknown = Some(format!(
                "a symlink points at `{}`, which is not in the repository",
                probe.path
            ));
            return;
        }
        for (path, sha) in links {
            self.follow(&path, &sha, queue).await;
            if self.unknown.is_some() {
                return;
            }
        }
        if !probe.tree {
            self.match_paths(None, vec![probe.path.clone()]).await;
            return;
        }
        // A lease spelled in another case lists nothing here but names a folder on a case-folding file system: what is under
        // it cannot be listed, so every class counts.
        if !found_any && let Some(real) = self.case_variant(&probe.path).await {
            if self.unknown.is_none() {
                self.unknown = Some(format!(
                    "the lease `{}/**` names `{real}` only when case is ignored",
                    probe.path
                ));
            }
            return;
        }
        // The files tracked under it: each is matched, and each symlink is followed.
        let Some(tracked) = self
            .list(std::slice::from_ref(&probe.path), true, self.limits.tracked)
            .await
        else {
            return;
        };
        if tracked.len() > self.limits.tracked {
            self.unknown = Some(format!(
                "more than {} files are tracked under `{}/**`",
                self.limits.tracked, probe.path
            ));
            return;
        }
        let mut names = Vec::with_capacity(tracked.len());
        for entry in tracked {
            if entry.mode == "120000" {
                self.follow(&entry.path, &entry.sha, queue).await;
                if self.unknown.is_some() {
                    return;
                }
            }
            names.push(entry.path);
        }
        // The lease itself too: a glob that can match under the directory may match a file a worker adds.
        self.match_paths(Some(probe.path.clone()), names).await;
    }

    /// Charges the work of `paths` to the brief's budget, then matches them off the async runtime. Over the budget the brief
    /// is unknown: slow work ends as every class, never as none.
    async fn match_paths(&mut self, dir: Option<String>, paths: Vec<String>) {
        self.paths_done += paths.len().max(1);
        self.tests_done += (paths.len() + 1) * self.globs.max(1);
        if self.paths_done > self.limits.paths || self.tests_done > self.limits.tests {
            self.unknown = Some(format!(
                "the brief's files are too many to check against {} patterns",
                self.globs
            ));
            return;
        }
        let file = self.file.clone();
        let done =
            tokio::task::spawn_blocking(move || match_batch(&file, dir.as_deref(), &paths)).await;
        match done {
            Ok(found) => {
                for one in found {
                    for sample in &one.samples {
                        self.add(one.rule, &one.pattern, sample);
                    }
                }
            }
            Err(_) => self.unknown = Some("the classes could not be checked".to_owned()),
        }
    }

    /// The name a folder really has when `path` only matches it with case or normalisation ignored. `None` when the path names
    /// nothing at all (a folder the worker would create) or is spelled as it is.
    async fn case_variant(&mut self, path: &str) -> Option<String> {
        let mut prefix = String::new();
        for part in path.split('/') {
            let at = if prefix.is_empty() {
                Vec::new()
            } else {
                vec![format!("{prefix}/")]
            };
            let children = self.list(&at, false, usize::MAX).await?;
            let names: Vec<&str> = children
                .iter()
                .map(|e| e.path.rsplit('/').next().unwrap_or(&e.path))
                .collect();
            if names.contains(&part) {
                if !prefix.is_empty() {
                    prefix.push('/');
                }
                prefix.push_str(part);
                continue;
            }
            let want = fold(part);
            return names
                .iter()
                .find(|name| fold(name) == want)
                .map(|name| (*name).to_owned());
        }
        None
    }

    /// Reads a symlink's target and queues what it reaches.
    async fn follow(&mut self, link: &str, sha: &str, queue: &mut Vec<Probe>) {
        if !self.seen.insert(format!("link|{link}")) {
            return;
        }
        self.links += 1;
        if self.links > LINKS_MAX {
            self.unknown = Some(format!(
                "more than {LINKS_MAX} symlinks lead out of the lease"
            ));
            return;
        }
        let target = match git(self.root, &["cat-file", "-s", sha]).await {
            Ok(size) if text(&size).parse::<usize>().is_ok_and(|n| n <= TARGET_MAX) => {
                git(self.root, &["cat-file", "blob", sha]).await.ok()
            }
            _ => None,
        };
        let Some(target) = target.and_then(|t| String::from_utf8(t).ok()) else {
            self.unknown = Some(format!("the symlink `{link}` could not be read"));
            return;
        };
        match resolve(link, &target) {
            Ok(path) => queue.push(Probe {
                path,
                tree: true,
                via_link: true,
            }),
            Err(why) => self.unknown = Some(format!("the symlink `{link}` {why}")),
        }
    }

    /// `ls-tree -z` of these paths at the base commit, literal. `recursive` lists the files under a directory. `None` when git
    /// cannot say (every class then counts, through `unknown`).
    async fn list(
        &mut self,
        paths: &[String],
        recursive: bool,
        most: usize,
    ) -> Option<Vec<Listed>> {
        let mut args: Vec<&str> = vec!["--literal-pathspecs", "ls-tree", "-z"];
        if recursive {
            args.push("-r");
        }
        args.push(self.base);
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
        let out = match git(self.root, &args).await {
            Ok(out) => out,
            Err(error) => {
                self.unknown = Some(format!(
                    "git could not list the files: {}",
                    error.public_message()
                ));
                return None;
            }
        };
        let mut listed = Vec::new();
        for part in out.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            let parsed = std::str::from_utf8(part).ok().and_then(|line| {
                let (meta, path) = line.split_once('\t')?;
                let mut meta = meta.split(' ');
                Some(Listed {
                    mode: meta.next()?.to_owned(),
                    sha: {
                        meta.next()?;
                        meta.next()?.to_owned()
                    },
                    path: path.to_owned(),
                })
            });
            match parsed {
                Some(entry) => listed.push(entry),
                None => {
                    self.unknown = Some("a tracked file has a name that is not text".to_owned());
                    return None;
                }
            }
            if listed.len() > most {
                break;
            }
        }
        Some(listed)
    }
}

struct Listed {
    mode: String,
    sha: String,
    path: String,
}

/// Where a symlink at `link` pointing to `target` leads, as a path inside the repository.
fn resolve(link: &str, target: &str) -> Result<String, &'static str> {
    if target.is_empty() || path_problem_target(target) {
        return Err("points outside the repository or somewhere that cannot be told");
    }
    let mut parts: Vec<&str> = link.split('/').collect();
    parts.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("points outside the repository");
                }
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return Err("points at the whole repository");
    }
    Ok(parts.join("/"))
}

fn path_problem_target(target: &str) -> bool {
    let bytes = target.as_bytes();
    target.starts_with('/')
        || target.contains('\\')
        || target.chars().any(char::is_control)
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn a_symlink_target_is_resolved_inside_the_tree_or_refused() {
        assert_eq!(resolve("src/a/link", "../b/x").unwrap(), "src/b/x");
        assert_eq!(resolve("link", "docs").unwrap(), "docs");
        assert_eq!(resolve("a/link", "./c//d").unwrap(), "a/c/d");
        for (link, target) in [
            ("link", "../outside"),
            ("a/link", "../../x"),
            ("a/link", "/etc/passwd"),
            ("a/link", "C:/x"),
            ("a/link", ".."),
            ("a/link", ""),
            ("a/link", "a\\b"),
        ] {
            assert!(resolve(link, target).is_err(), "{link} -> {target}");
        }
    }
}
