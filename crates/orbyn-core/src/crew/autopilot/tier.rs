//! How much approval an action that is not always denied needs. Three answers, in code: nothing
//! (reads and writes inside the worker's leased paths, the brief's own checks), the lead alone, or
//! the lead and a second key on another vendor. The second key is for network, installing
//! dependencies, running generated scripts and touching CI files (docs/crew.md, "Autopilot").

use orbyn_contracts::{ActionKind, CrewPathPattern};

use super::hard::{Ctx, written};
use super::shell::{Simple, hosts, outside, relative};
use crate::crew::checks::covers;

/// What an action needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Always allowed: no approver is asked.
    Nothing,
    /// The lead decides, with a reason. The ledger calls this tier `low`.
    Lead,
    /// The lead and an approver on another vendor both say yes. The ledger calls this tier `medium`.
    TwoKeys,
}

impl Need {
    pub fn tier(self) -> &'static str {
        match self {
            Self::Nothing => "always_allowed",
            Self::Lead => "low",
            Self::TwoKeys => "medium",
        }
    }
}

/// What the worker was given: the paths it may write and the commands of its own checks.
pub struct Grants<'a> {
    pub writes: &'a [CrewPathPattern],
    pub check_commands: &'a [String],
}

/// The need, and the reasons that raised it above the lead alone.
pub struct Tiered {
    pub need: Need,
    pub reasons: Vec<&'static str>,
}

const CI_FILES: [&str; 9] = [
    ".github/workflows/",
    ".github/actions/",
    ".gitlab-ci.yml",
    ".circleci/",
    "jenkinsfile",
    "azure-pipelines.yml",
    ".travis.yml",
    ".buildkite/",
    ".drone.yml",
];

pub(super) fn is_ci(path: &str) -> bool {
    let lower = path.to_lowercase();
    CI_FILES
        .iter()
        .any(|ci| lower == *ci || lower.starts_with(ci) || lower.ends_with(&format!("/{ci}")))
}

const NETWORK_PROGRAMS: [&str; 14] = [
    "curl", "wget", "http", "https", "xh", "nc", "ncat", "netcat", "telnet", "ftp", "ssh", "sftp",
    "scp", "mosh",
];

fn installs(command: &Simple) -> bool {
    let words: Vec<&str> = command
        .args
        .iter()
        .map(String::as_str)
        .filter(|w| !w.starts_with('-'))
        .collect();
    let first = words.first().copied();
    let in_pip = command
        .args
        .windows(2)
        .any(|w| w[0] == "pip" || w[1] == "pip")
        && words.contains(&"install");
    match command.program.as_str() {
        "npm" | "pnpm" | "yarn" | "bun" => {
            matches!(
                first,
                Some(
                    "install"
                        | "i"
                        | "add"
                        | "ci"
                        | "update"
                        | "upgrade"
                        | "up"
                        | "link"
                        | "dlx"
                        | "exec"
                        | "x"
                )
            ) || command.args.is_empty() && command.program == "yarn"
        }
        "npx" | "pnpx" | "bunx" | "pipx" | "uvx" => true,
        "pip" | "pip3" | "uv" | "conda" | "mamba" => {
            matches!(first, Some("install" | "add" | "sync"))
        }
        "python" | "python3" => in_pip,
        "cargo" => matches!(
            first,
            Some("add" | "install" | "fetch" | "update" | "vendor")
        ),
        "go" => {
            matches!(first, Some("get" | "install"))
                || first == Some("mod")
                    && words
                        .get(1)
                        .is_some_and(|w| matches!(*w, "download" | "tidy" | "vendor"))
        }
        "gem" | "brew" | "apt" | "apt-get" | "yum" | "dnf" | "apk" | "pacman" | "choco"
        | "winget" | "scoop" | "nuget" => {
            matches!(first, Some("install" | "add" | "upgrade" | "update"))
        }
        "bundle" | "poetry" | "composer" | "dotnet" => {
            matches!(
                first,
                Some("install" | "add" | "require" | "restore" | "update")
            )
        }
        _ => false,
    }
}

fn pulls_remote(command: &Simple) -> bool {
    command.program == "git"
        && command.args.iter().any(|w| {
            matches!(
                w.as_str(),
                "clone" | "fetch" | "pull" | "push" | "ls-remote" | "submodule" | "remote"
            )
        })
}

fn generated(command: &Simple, ctx: &Ctx<'_>, grants: &Grants<'_>) -> bool {
    let tree = ctx.scene.worktree;
    let in_lease = |word: &str| {
        relative(word, tree).is_some_and(|rel| grants.writes.iter().any(|p| covers(p, &rel)))
    };
    let shell = matches!(
        command.program.as_str(),
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish"
    );
    let interpreter = shell
        || matches!(
            command.program.as_str(),
            "python"
                | "python3"
                | "node"
                | "deno"
                | "ruby"
                | "perl"
                | "php"
                | "lua"
                | "tsx"
                | "ts-node"
                | "pwsh"
                | "powershell"
        );
    if interpreter {
        let inline = command.args.iter().any(|a| {
            matches!(
                a.as_str(),
                "-c" | "-e" | "-p" | "-r" | "-command" | "-Command" | "--eval" | "--print" | "-lc"
            )
        });
        let file = command.args.iter().find(|a| !a.starts_with('-'));
        // A shell with no arguments reads a script from its input: `curl ... | sh`.
        let piped = shell && command.args.is_empty();
        return inline || piped || file.is_some_and(|f| in_lease(f));
    }
    command.raw.contains('/') && in_lease(&command.raw)
}

const READ_ONLY: [&str; 24] = [
    "ls", "cat", "head", "tail", "wc", "pwd", "echo", "rg", "grep", "egrep", "fgrep", "tree",
    "stat", "file", "diff", "sort", "uniq", "cut", "tr", "basename", "dirname", "realpath",
    "which", "true",
];
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

fn read_only(command: &Simple, ctx: &Ctx<'_>) -> bool {
    if !command.redirects.is_empty() {
        return false;
    }
    let tree = ctx.scene.worktree;
    let inside = command
        .args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .all(|a| !super::shell::is_path_like(a) || !outside(a, tree));
    let program_ok = match command.program.as_str() {
        p if READ_ONLY.contains(&p) => true,
        "find" => !command.args.iter().any(|a| {
            matches!(
                a.as_str(),
                "-exec" | "-execdir" | "-ok" | "-delete" | "-fprint" | "-fprintf"
            )
        }),
        "sed" => !command.args.iter().any(|a| a.starts_with("-i")),
        "git" => command
            .args
            .iter()
            .find(|a| !a.starts_with('-'))
            .is_some_and(|s| GIT_READS.contains(&s.as_str())),
        _ => false,
    };
    program_ok && inside
}

/// The approval an action needs. The always-denied list has already passed.
pub(super) fn tier(ctx: &Ctx<'_>, grants: &Grants<'_>) -> Tiered {
    let mut reasons: Vec<&'static str> = Vec::new();
    if written(ctx).iter().any(|p| is_ci(p)) {
        reasons.push("it touches a CI file");
    }
    if ctx.kind == ActionKind::Network
        || ctx.kind == ActionKind::Browser
        || ctx.commands.iter().any(|c| {
            !hosts(c).is_empty()
                || NETWORK_PROGRAMS.contains(&c.program.as_str())
                || pulls_remote(c)
        })
    {
        reasons.push("it uses the network");
    }
    if ctx.commands.iter().any(installs) {
        reasons.push("it installs dependencies");
    }
    if ctx.commands.iter().any(|c| generated(c, ctx, grants)) {
        reasons.push("it runs a script or code the worker wrote");
    }
    if !reasons.is_empty() {
        return Tiered {
            need: Need::TwoKeys,
            reasons,
        };
    }
    let always = match ctx.kind {
        ActionKind::FileChange => {
            let inside = written(ctx);
            let wanted = if ctx.paths.is_empty() {
                1
            } else {
                ctx.paths.len()
            };
            !inside.is_empty()
                && inside.len() == wanted
                && inside
                    .iter()
                    .all(|p| grants.writes.iter().any(|g| covers(g, p)))
        }
        ActionKind::Command => {
            let text = ctx.text.lines().next().unwrap_or("").trim();
            grants.check_commands.iter().any(|c| c.trim() == text)
                || !ctx.commands.is_empty() && ctx.commands.iter().all(|c| read_only(c, ctx))
        }
        _ => false,
    };
    Tiered {
        need: if always { Need::Nothing } else { Need::Lead },
        reasons,
    }
}

/// The need of an action, for a caller that has no `Core`: tests and the dry run.
pub fn tier_of(
    kind: ActionKind,
    target: &str,
    paths: &[String],
    scene: &super::hard::Scene<'_>,
    grants: &Grants<'_>,
) -> Need {
    tier(&super::hard::context(kind, target, paths, scene), grants).need
}
