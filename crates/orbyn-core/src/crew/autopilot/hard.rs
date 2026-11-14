//! The always-denied list, enforced in code (SPEC.md section 7.1, N1 to N10). It runs before any
//! approver, rule or mode, so no lead, second key or person's saved rule can allow what it
//! refuses, and the lead is never even asked. It reads the action's kind, target and paths with
//! the checks in [`shell`](super::shell); a refusal names the rule and never the match.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use orbyn_contracts::{ActionKind, CrewPathPattern};
use regex::Regex;
use serde_json::json;

use super::shell::{self, Simple, host_allowed, hosts, outside, outside_from, relative};
use crate::account::changes::{account_paths_in, changes_account_state};
use crate::crew::changes::crew_change;
use crate::crew::checks::covers;
use crate::crew::find_secret;
use crate::redaction::changes::redaction_change;

/// What the rules need to know besides the action.
pub struct Scene<'a> {
    pub worktree: &'a Path,
    /// The answer key's protected paths (N7).
    pub protected: &'a [CrewPathPattern],
    /// Hosts the repository allows, from `.orbyn/crew.json` at the base commit (N3).
    pub allowed_hosts: &'a [String],
    /// Orbyn's own data folder, which holds the store and the keys (N2).
    pub data_dir: Option<&'a Path>,
    /// The branch the worktree is on; `None` is read as the default branch (N5).
    pub branch: Option<&'a str>,
    /// The repository's default branch (`origin/HEAD`), when it is known; main, master and trunk are always read as main.
    pub default_branch: Option<&'a str>,
}

/// Why an action is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub id: &'static str,
    pub rule: &'static str,
}

/// The names of the rules, in the order of SPEC.md section 7.1.
pub const RULES: [(&str, &str); 10] = [
    ("N1", "changing account, device or sign-in state"),
    ("N2", "reading or sending secrets and credentials"),
    (
        "N3",
        "network access to a host the repository has not allowed",
    ),
    ("N4", "writing or deleting outside the worktree"),
    ("N5", "force-push, history rewrite or any push to main"),
    ("N6", "changing branch protection, permissions or settings"),
    ("N7", "editing answer keys, check files or .orbyn/crew.json"),
    ("N8", "production deploys"),
    ("N9", "publishing or messaging outside the repository"),
    ("N10", "payments"),
];

type Rule = fn(&Ctx<'_>) -> bool;

/// What every rule reads.
pub(super) struct Ctx<'a> {
    pub kind: ActionKind,
    pub target: String,
    pub text: String,
    pub paths: &'a [String],
    pub commands: Vec<Simple>,
    pub scene: &'a Scene<'a>,
}

/// The first rule an action breaks, if any. The order puts the account and the secrets first.
pub fn refuse(
    kind: ActionKind,
    target: &str,
    paths: &[String],
    scene: &Scene<'_>,
) -> Option<Refusal> {
    refuse_in(&context(kind, target, paths, scene))
}

/// What the rules and the tiers read about an action.
pub(super) fn context<'a>(
    kind: ActionKind,
    target: &str,
    paths: &'a [String],
    scene: &'a Scene<'a>,
) -> Ctx<'a> {
    let commands = if kind == ActionKind::Command {
        shell::simple_commands(target)
    } else {
        Vec::new()
    };
    let text = std::iter::once(target)
        .chain(paths.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    Ctx {
        kind,
        target: target.to_owned(),
        text,
        paths,
        commands,
        scene,
    }
}

pub(super) fn refuse_in(ctx: &Ctx<'_>) -> Option<Refusal> {
    let order: [(usize, Rule); 10] = [
        (0, n1),
        (1, n2),
        (6, n7),
        (3, n4),
        (4, super::hard_tools::n5),
        (5, super::hard_tools::n6),
        (7, super::hard_tools::n8),
        (8, super::hard_tools::n9),
        (9, super::hard_tools::n10),
        (2, n3),
    ];
    order
        .iter()
        .find(|(_, rule)| rule(ctx))
        .map(|(at, _)| Refusal {
            id: RULES[*at].0,
            rule: RULES[*at].1,
        })
}

/// Programs that open a connection, or run code that can: the ones an account route can be reached with.
const CONNECTING: &[&str] = &[
    "gh",
    "glab",
    "socat",
    "openssl",
    "curl",
    "wget",
    "http",
    "https",
    "xh",
    "httpie",
    "nc",
    "ncat",
    "netcat",
    "telnet",
    "powershell",
    "pwsh",
    "python",
    "python3",
    "node",
    "deno",
    "bun",
    "ruby",
    "perl",
    "php",
    "sh",
    "bash",
];

/// Whether an action reaches a state-changing account route. A file's path, a search, a read of a document that
/// merely names the route are not a request to it: only a network, browser or tool action, or a command run by a
/// connecting program, is looked at.
pub fn account_refused(kind: ActionKind, target: &str, paths: &[String]) -> bool {
    match kind {
        ActionKind::FileChange => false,
        // A connecting program anywhere makes the whole text a request: the route can come through a variable
        // (`U=http://.../api/account/sign-out; curl -X POST $U`) or a pipe (`printf 'POST /api/... ' | nc ...`).
        // The text is read raw and again word by word with the quotes taken off, so `/api/"crew"/...` is seen.
        ActionKind::Command => {
            let commands = shell::simple_commands(target);
            commands
                .iter()
                .any(|command| CONNECTING.contains(&command.program.as_str()))
                && (account_change(target)
                    || commands.iter().any(|command| {
                        std::iter::once(&command.raw)
                            .chain(&command.args)
                            .chain(&command.redirects)
                            .any(|word| account_change(word))
                    }))
        }
        _ => {
            let text = std::iter::once(target)
                .chain(paths.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            account_change(&text)
        }
    }
}

/// `text` with `//`, `/./` and `/x/../` squashed, so a path that is spelled around its prefix is still found. Spelling is all it
/// reads: a path built from variables, string pieces or a written script is not seen (see the README).
fn squash(text: &str) -> String {
    static UP: OnceLock<Regex> = OnceLock::new();
    let up = UP.get_or_init(|| Regex::new(r"/[^/\s]+/\.\./").expect("a fixed pattern"));
    let mut text = text.to_owned();
    for _ in 0..8 {
        let next = up
            .replace_all(&text, "/")
            .replace("/./", "/")
            .replace("//", "/");
        if next == text {
            break;
        }
        text = next;
    }
    text
}

/// Whether text names a state-changing account route, or a Crew or Settings › Secrets route that changes state: the person's answers are not an
/// agent's to give (`crew::changes`).
pub fn account_change(text: &str) -> bool {
    let named = |text: &str| {
        account_paths_in(text)
            .iter()
            .any(|path| changes_account_state(path))
            || crew_change(text)
            || redaction_change(text)
    };
    named(text) || named(&squash(text))
}

/// N1: a request to a state-changing account route, or to one this module does not know, and likewise a state-changing Crew
/// route. **No agent approves; a person may approve once:** the routes that answer a held command (`approve-once`, `deny`,
/// `dismiss`) are Crew routes that change state, so an agent's request to one is refused here, in every mode, and the person's
/// own call needs the desktop app's grant (see `held`). Nothing on this list is ever held, and a person's approval cannot
/// cover it: it is checked again at approval time.
fn n1(ctx: &Ctx<'_>) -> bool {
    account_refused(ctx.kind, &ctx.target, ctx.paths)
}

fn secret_names() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)[$%]\{?[A-Z0-9_]*(TOKEN|SECRET|PASSWORD|PASSWD|API_?KEY|PRIVATE_?KEY|CREDENTIAL)[A-Z0-9_]*\}?%?|(^|[;&|]\s*)(env|printenv|export\s+-p|set)\s*($|[|;&>])")
            .expect("a fixed pattern")
    })
}

const SECRET_DIRS: [&str; 10] = [
    ".ssh",
    ".aws",
    ".gnupg",
    ".kube",
    ".azure",
    ".netrc",
    "_netrc",
    ".npmrc",
    ".pypirc",
    ".git-credentials",
];
const SECRET_EXT: [&str; 7] = ["pem", "p12", "pfx", "key", "keystore", "kdbx", "jks"];
const SECRET_FILES: [&str; 6] = [
    "secrets.yml",
    "secrets.yaml",
    "secrets.json",
    "credentials.json",
    "token.json",
    "service-account.json",
];

/// Whether a path word names a file that usually holds a credential.
pub(super) fn secret_path(word: &str) -> bool {
    let lower = word.to_lowercase().replace('\\', "/");
    let parts: Vec<&str> = lower.split('/').filter(|p| !p.is_empty()).collect();
    let Some(base) = parts.last() else {
        return false;
    };
    if parts.iter().any(|p| SECRET_DIRS.contains(p)) {
        return true;
    }
    let pair = |a: &str, b: &str| parts.windows(2).any(|w| w[0] == a && w[1] == b);
    if pair(".config", "gh") || pair(".config", "gcloud") || pair(".docker", "config.json") {
        return true;
    }
    if ["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"]
        .iter()
        .any(|k| base.starts_with(k))
        || SECRET_FILES.contains(base)
        || base
            .rsplit_once('.')
            .is_some_and(|(_, ext)| SECRET_EXT.contains(&ext))
    {
        return true;
    }
    base.strip_prefix(".env").is_some_and(|rest| {
        rest.is_empty()
            || rest.strip_prefix('.').is_some_and(|ext| {
                !["example", "sample", "template", "dist", "defaults"].contains(&ext)
            })
    })
}

/// N2: a secret in the text, a credential file or the data folder named, a program that prints one.
fn n2(ctx: &Ctx<'_>) -> bool {
    if ctx.kind != ActionKind::FileChange && find_secret(&json!(ctx.text)).is_some() {
        return true;
    }
    if ctx.kind == ActionKind::Command && secret_names().is_match(&ctx.text) {
        return true;
    }
    if let Some(data) = ctx.scene.data_dir.and_then(Path::to_str)
        && !data.is_empty()
        && ctx.text.contains(data)
    {
        return true;
    }
    ctx.commands.iter().any(|command| {
        let words: Vec<&str> = command.args.iter().map(String::as_str).collect();
        let sub = words.iter().find(|w| !w.starts_with('-')).copied();
        let program_secret = match command.program.as_str() {
            "security" => matches!(
                sub,
                Some(
                    "find-generic-password" | "find-internet-password" | "dump-keychain" | "export"
                )
            ),
            "secret-tool" => matches!(sub, Some("lookup" | "search")),
            "pass" | "op" | "gopass" | "vault" => true,
            "gh" => {
                sub == Some("auth")
                    && words
                        .iter()
                        .any(|w| matches!(*w, "token" | "--show-token" | "-t"))
            }
            "gcloud" => words
                .iter()
                .any(|w| w.starts_with("print-") && w.ends_with("-token")),
            "az" => words.contains(&"get-access-token"),
            "aws" => words.contains(&"get-session-token") || words.contains(&"export-credentials"),
            "kubectl" => words.contains(&"view") && words.contains(&"--raw"),
            "printenv" => true,
            _ => false,
        };
        program_secret
            || command
                .args
                .iter()
                .chain(&command.redirects)
                .flat_map(|arg| arg.split(['=', '@']))
                .any(secret_path)
    })
}

const GIT_READS: [&str; 12] = [
    "status",
    "diff",
    "log",
    "show",
    "ls-files",
    "rev-parse",
    "blame",
    "grep",
    "cat-file",
    "describe",
    "shortlog",
    "ls-tree",
];

/// `-C` folder, subcommand, words that are not options, every argument after the subcommand.
type GitParts = (Option<String>, String, Vec<String>, Vec<String>);

/// A git command split into its `-C` folder, its subcommand, the words after it that are not options, and every argument after it.
fn git_parts(command: &Simple) -> Option<GitParts> {
    let mut rest = command.args.iter();
    let mut dir = None;
    let mut sub = None;
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "-C" => dir = rest.next().cloned(),
            "-c" | "--git-dir" | "--work-tree" | "--namespace" => {
                rest.next();
            }
            flag if flag.starts_with('-') => {}
            other => {
                sub = Some(other.to_owned());
                break;
            }
        }
    }
    let after: Vec<String> = rest.cloned().collect();
    let words: Vec<String> = after
        .iter()
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect();
    Some((dir, sub?, words, after))
}

/// The files a command writes or deletes, as the words it names them by.
pub(super) fn writes(command: &Simple) -> Vec<String> {
    let args: Vec<&String> = command
        .args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .collect();
    let mut out: Vec<String> = command.redirects.clone();
    let flag = |f: &str| {
        command
            .args
            .iter()
            .any(|a| a == f || a.starts_with(&format!("{f}.")))
    };
    let after = |f: &str| {
        command
            .args
            .iter()
            .position(|a| a == f)
            .and_then(|at| command.args.get(at + 1))
            .cloned()
    };
    match command.program.as_str() {
        "rm" | "rmdir" | "unlink" | "shred" | "truncate" | "touch" | "mkdir" | "chmod"
        | "chown" | "chgrp" | "mv" | "tee" | "ln" | "del" | "erase" | "rd" | "remove-item"
        | "move-item" | "new-item" | "set-content" => out.extend(args.iter().map(|a| (*a).clone())),
        "cp" | "install" | "rsync" | "scp" | "copy" | "copy-item" => {
            let target = after("-t")
                .or_else(|| {
                    command
                        .args
                        .iter()
                        .find_map(|a| a.strip_prefix("--target-directory=").map(str::to_owned))
                })
                .or_else(|| args.last().map(|a| (*a).clone()));
            out.extend(target);
        }
        "sed" | "perl" | "ruby" if flag("-i") || flag("-pi") || flag("-ni") => {
            out.extend(args.iter().map(|a| (*a).clone()));
        }
        "dd" => out.extend(
            command
                .args
                .iter()
                .filter_map(|a| a.strip_prefix("of=").map(str::to_owned)),
        ),
        "find" => {
            if command.args.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir"
                )
            }) {
                out.extend(
                    command
                        .args
                        .iter()
                        .take_while(|a| !a.starts_with(['-', '(', '!']))
                        .cloned(),
                );
            }
        }
        "tar" | "bsdtar" => {
            out.extend(after("-C"));
            out.extend(after("--directory"));
            out.extend(
                command
                    .args
                    .iter()
                    .filter_map(|a| a.strip_prefix("--directory=").map(str::to_owned)),
            );
        }
        "unzip" => out.extend(after("-d")),
        "git" => {
            let Some((dir, sub, words, _)) = git_parts(command) else {
                return out;
            };
            if !GIT_READS.contains(&sub.as_str()) {
                // `git -C ../other <anything that changes>` acts on another repository.
                out.extend(dir);
            }
            match sub.as_str() {
                "checkout" | "restore" | "rm" | "mv" | "clean" => out.extend(words),
                "worktree" if matches!(words.first().map(String::as_str), Some("add" | "move")) => {
                    out.extend(words.into_iter().skip(1));
                }
                "init" | "clone" => out.extend(words.last().cloned()),
                _ => {}
            }
        }
        _ => {}
    }
    out
}

/// A written word and the directory it is read from: `None` once a `cd` to somewhere unknown has happened.
pub(super) fn write_targets(ctx: &Ctx<'_>) -> Vec<(String, Option<PathBuf>)> {
    let mut cwd = Some(ctx.scene.worktree.to_path_buf());
    let mut out = Vec::new();
    for command in &ctx.commands {
        if matches!(command.program.as_str(), "cd" | "pushd" | "chdir" | "popd") {
            let word = command
                .args
                .iter()
                .find(|a| !a.starts_with('-'))
                .map(String::as_str);
            cwd = shell::change_dir(cwd.as_deref(), word);
            continue;
        }
        out.extend(writes(command).into_iter().map(|w| (w, cwd.clone())));
    }
    out
}

/// N4: a write or delete that lands outside the worktree.
fn n4(ctx: &Ctx<'_>) -> bool {
    let tree = ctx.scene.worktree;
    if ctx.kind == ActionKind::FileChange {
        return ctx.paths.iter().any(|p| outside(p, tree))
            || (ctx.paths.is_empty() && outside(ctx.text.lines().next().unwrap_or(""), tree));
    }
    write_targets(ctx)
        .iter()
        .any(|(word, cwd)| outside_from(word, cwd.as_deref(), tree))
}

/// The files the action writes, as worktree-relative paths where they lie inside it.
pub(super) fn written(ctx: &Ctx<'_>) -> Vec<String> {
    let tree = ctx.scene.worktree;
    if ctx.kind == ActionKind::FileChange {
        let words: Vec<String> = if ctx.paths.is_empty() {
            ctx.text.lines().take(1).map(str::to_owned).collect()
        } else {
            ctx.paths.to_vec()
        };
        return words.iter().filter_map(|w| relative(w, tree)).collect();
    }
    write_targets(ctx)
        .iter()
        .filter_map(|(w, cwd)| shell::relative_from(w, cwd.as_deref(), tree))
        .collect()
}

/// N7: a write to `.orbyn/crew.json` (or the `.orbyn` folder it is in) or to a path the answer key protects, a git command that
/// removes or changes the file in a commit, the index or the checkout, and a git command that moves a ref the rules are read at.
fn n7(ctx: &Ctx<'_>) -> bool {
    written(ctx).iter().any(|path| {
        path == crate::crew::RULES_FILE
            || path == RULES_DIR
            || ctx
                .scene
                .protected
                .iter()
                .any(|pattern| covers(pattern, path))
    }) || git_changes_rules(ctx)
}

/// The folder of the rules file.
const RULES_DIR: &str = ".orbyn";

/// Git subcommands that can put a path into a commit, the index or the checkout, or take it out.
const GIT_CHANGES: [&str; 20] = [
    "add",
    "commit",
    "reset",
    "restore",
    "checkout",
    "switch",
    "stash",
    "rm",
    "mv",
    "clean",
    "apply",
    "am",
    "update-index",
    "read-tree",
    "checkout-index",
    "merge",
    "rebase",
    "cherry-pick",
    "revert",
    "pull",
];

/// Options of the subcommands above that take a value in the next argument, so the value is not read as a path or a revision.
const GIT_VALUE_OPTIONS: [&str; 16] = [
    "-m",
    "-F",
    "-C",
    "-c",
    "-b",
    "-B",
    "--message",
    "--file",
    "--author",
    "--date",
    "--source",
    "--pathspec-from-file",
    "--reuse-message",
    "--reedit-message",
    "-s",
    "--strategy",
];

/// What follows `--` and what comes before it, after the values of options are set aside. `source` is whether `--source` was given.
struct GitWords {
    before: Vec<String>,
    after: Vec<String>,
    source: bool,
}

fn git_words(args: &[String]) -> GitWords {
    let mut out = GitWords {
        before: Vec::new(),
        after: Vec::new(),
        source: false,
    };
    let mut dashes = false;
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if dashes {
            out.after.push(arg.clone());
        } else if arg == "--" {
            dashes = true;
        } else if arg.starts_with("--source") {
            out.source = true;
            skip = arg == "--source";
        } else if arg.starts_with('-') {
            skip = GIT_VALUE_OPTIONS.contains(&arg.as_str());
        } else {
            out.before.push(arg.clone());
        }
    }
    out
}

/// Whether a refspec's destination is under `refs/remotes/`, by git's own expansion (`remotes/x` is `refs/remotes/x`), compared
/// without case because a case-insensitive file system treats `refs/REMOTES` as the same folder. A destination spelled
/// `origin/...` could also resolve there, so it counts.
fn into_remote_refs(word: &str) -> bool {
    let spec = word.trim_start_matches('+');
    let dest = spec.rsplit_once(':').map_or(spec, |(_, dest)| dest);
    if dest.is_empty() || (!word.contains(':') && !dest.contains('/')) {
        return false;
    }
    let lower = dest.to_ascii_lowercase();
    lower.starts_with("refs/remotes/")
        || lower.starts_with("remotes/")
        || (word.contains(':') && lower.starts_with("origin/"))
}

/// Whether a git command moves, repoints or deletes a ref the rules are also read at.
fn moves_rules_ref(sub: &str, after: &[String], words: &[String]) -> bool {
    let has = |names: &[&str]| after.iter().any(|a| names.contains(&a.as_str()));
    match sub {
        // Ref plumbing and bulk writers.
        "update-ref" | "fast-import" | "send-pack" => true,
        // Reading a symbolic ref has one name; a write names the ref and its target, or deletes.
        "symbolic-ref" => words.len() >= 2 || has(&["-d", "--delete"]),
        "fetch" | "push" | "pull" => has(&["--stdin"]) || words.iter().any(|w| into_remote_refs(w)),
        // The remote the pinned ref belongs to.
        "remote" => matches!(
            words.first().map(String::as_str),
            Some("set-head" | "rename" | "remove" | "rm" | "add" | "set-url" | "set-branches")
        ),
        "branch" => {
            has(&["-r", "--remotes"])
                && has(&[
                    "-d", "-D", "-m", "-M", "-c", "-C", "--delete", "--move", "--copy",
                ])
        }
        _ => false,
    }
}

/// Whether a git command in the action names the rules file (or its folder, or a whole checkout together with a revision, which
/// takes the file from that revision) to change it, or moves a ref the rules are also read at. A bare `git checkout <revision>`
/// names no path, so it is not seen here: the default branch's copy of the file is the check for that.
fn git_changes_rules(ctx: &Ctx<'_>) -> bool {
    let tree = ctx.scene.worktree;
    let mut cwd = Some(tree.to_path_buf());
    for command in &ctx.commands {
        if matches!(command.program.as_str(), "cd" | "pushd" | "chdir" | "popd") {
            let word = command
                .args
                .iter()
                .find(|a| !a.starts_with('-'))
                .map(String::as_str);
            cwd = shell::change_dir(cwd.as_deref(), word);
            continue;
        }
        if command.program != "git" {
            continue;
        }
        let Some((_, sub, words, after)) = git_parts(command) else {
            continue;
        };
        let sub = sub.as_str();
        if moves_rules_ref(sub, &after, &words) {
            return true;
        }
        if !GIT_CHANGES.contains(&sub) {
            continue;
        }
        let parsed = git_words(&after);
        let from_revision = matches!(sub, "checkout" | "restore" | "reset")
            && (parsed.source
                || (!parsed.after.is_empty() && !parsed.before.is_empty())
                || (parsed.after.is_empty() && parsed.before.len() > 1));
        let hit =
            parsed.before.iter().chain(&parsed.after).any(|word| {
                match shell::relative_from(word, cwd.as_deref(), tree) {
                    Some(path) if path == crate::crew::RULES_FILE || path == RULES_DIR => true,
                    Some(path) if path.is_empty() => sub == "rm" || from_revision,
                    _ => false,
                }
            });
        if hit {
            return true;
        }
    }
    false
}

/// N3: a host the repository has not allowed.
fn n3(ctx: &Ctx<'_>) -> bool {
    let from_commands = ctx.commands.iter().flat_map(hosts);
    let from_text: Vec<String> = if ctx.kind == ActionKind::Command {
        Vec::new()
    } else {
        let simple = Simple {
            program: String::new(),
            raw: String::new(),
            args: ctx.text.split_whitespace().map(str::to_owned).collect(),
            redirects: Vec::new(),
        };
        let mut found = hosts(&simple);
        // A network action's target is often a bare host.
        if ctx.kind == ActionKind::Network {
            let target = ctx.text.lines().next().unwrap_or("").trim();
            if !target.contains("://") {
                let host = target.split(['/', ':', '?']).next().unwrap_or("");
                if !host.is_empty() {
                    found.push(host.to_lowercase());
                }
            }
        }
        found
    };
    from_commands
        .chain(from_text)
        .any(|host| !host_allowed(&host, ctx.scene.allowed_hosts))
}
