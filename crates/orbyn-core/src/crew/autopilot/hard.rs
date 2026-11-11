//! The always-denied list, enforced in code (SPEC.md section 7.1, N1 to N10). It runs before any
//! approver, rule or mode, so no lead, second key or person's saved rule can allow what it
//! refuses, and the lead is never even asked. It reads the action's kind, target and paths with
//! the checks in [`shell`](super::shell); a refusal names the rule and never the match.

use std::path::Path;
use std::sync::OnceLock;

use orbyn_contracts::{ActionKind, CrewPathPattern};
use regex::Regex;
use serde_json::json;

use super::shell::{self, Simple, host_allowed, hosts, is_path_like, outside, relative};
use crate::account::changes::{account_paths_in, changes_account_state};
use crate::crew::checks::covers;
use crate::crew::find_secret;

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
    ("N5", "force-push or history rewrite on main"),
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

/// Whether text names a state-changing account route. The one rule enforced outside a Crew task.
pub fn account_change(text: &str) -> bool {
    account_paths_in(text)
        .iter()
        .any(|path| changes_account_state(path))
}

/// N1: a request to a state-changing account route, or to one this module does not know.
fn n1(ctx: &Ctx<'_>) -> bool {
    account_change(&ctx.text)
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
    match command.program.as_str() {
        "rm" | "rmdir" | "unlink" | "shred" | "truncate" | "touch" | "mkdir" | "chmod"
        | "chown" | "chgrp" | "mv" | "tee" | "ln" | "del" | "erase" | "rd" | "remove-item"
        | "move-item" | "new-item" | "set-content" => out.extend(args.iter().map(|a| (*a).clone())),
        "cp" | "install" | "rsync" | "scp" | "copy" | "copy-item" => {
            let target = command
                .args
                .iter()
                .position(|a| a == "-t")
                .and_then(|at| command.args.get(at + 1))
                .or_else(|| {
                    command
                        .args
                        .iter()
                        .find_map(|a| a.strip_prefix("--target-directory=").map(|_| a))
                })
                .cloned()
                .or_else(|| args.last().map(|a| (*a).clone()));
            out.extend(target.map(|t| t.trim_start_matches("--target-directory=").to_owned()));
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
        "git" => {
            let sub = args.first().map(|s| s.as_str());
            if matches!(
                sub,
                Some("checkout" | "restore" | "rm" | "mv" | "apply" | "clean")
            ) {
                out.extend(args.iter().skip(1).map(|a| (*a).clone()));
            }
        }
        _ => {}
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
    ctx.commands
        .iter()
        .flat_map(writes)
        .any(|word| (is_path_like(&word) || word.starts_with('~')) && outside(&word, tree))
}

/// The files the action writes, as worktree-relative paths where they lie inside it.
pub(super) fn written(ctx: &Ctx<'_>) -> Vec<String> {
    let tree = ctx.scene.worktree;
    let words: Vec<String> = if ctx.kind == ActionKind::FileChange {
        if ctx.paths.is_empty() {
            ctx.text.lines().take(1).map(str::to_owned).collect()
        } else {
            ctx.paths.to_vec()
        }
    } else {
        ctx.commands.iter().flat_map(writes).collect()
    };
    words.iter().filter_map(|w| relative(w, tree)).collect()
}

/// N7: a write to `.orbyn/crew.json` or to a path the answer key protects.
fn n7(ctx: &Ctx<'_>) -> bool {
    written(ctx).iter().any(|path| {
        path == crate::crew::RULES_FILE
            || ctx
                .scene
                .protected
                .iter()
                .any(|pattern| covers(pattern, path))
    })
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
