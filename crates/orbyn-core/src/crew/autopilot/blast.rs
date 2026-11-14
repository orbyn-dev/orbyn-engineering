//! The blast-radius guard (slice 4i): a destructive command against a production-linked target always needs a second key, even in
//! Full, and a person may always approve it alone, once. This module only reads a command: it finds the destructive category, the
//! targets the command reaches, and judges each target against this device's two lists. It runs nothing and changes nothing.
//!
//! **Heuristic, not a sandbox.** The categories and their patterns are built in (a repository file cannot add or remove one), but a
//! command is read as text: a shell wrapper the parser does not unwrap, an alias, a function, a script the command runs, or a file
//! given to `psql -f` is not read. The guard fails closed where it can see that it cannot tell: a target that is a variable, a tool
//! whose default target is remote and that names none, and an unreadable command that holds a destructive word and a non-local word.
//!
//! N8 (kubectl delete, terraform destroy, helm uninstall) and N3 stay above the guard in the review order: they are refused
//! before this module runs and are never held. A target is **local** when it is loopback, a unix socket, a file or a path (the
//! worktree is N4's), the local Docker socket or `default` context, or a git remote whose URL is a local path. Anything else is
//! production-linked unless this device's `safe` list names it; the `prod` list forces a target production-linked and wins over `safe`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use orbyn_contracts::{GuardCategory, GuardList, GuardTarget, GuardTargetKind};
use regex::Regex;

use super::hard::Ctx;
use super::shell::{Simple, simple_commands};

/// What the guard needs besides the command: this device's two lists and the worktree's git remotes (name to push URL).
#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub prod: Vec<GuardTarget>,
    pub safe: Vec<GuardTarget>,
    pub remotes: HashMap<String, String>,
}

impl Facts {
    /// Splits the stored entries into the two lists.
    pub fn new(entries: Vec<GuardTarget>, remotes: HashMap<String, String>) -> Self {
        let (prod, safe) = entries
            .into_iter()
            .partition(|entry| entry.list == GuardList::Prod);
        Self {
            prod,
            safe,
            remotes,
        }
    }
}

/// How one target was judged.
#[derive(Clone, Debug, PartialEq)]
pub enum Class {
    /// Loopback, a socket, a path or the local daemon.
    Local,
    /// On the `safe` list.
    Safe(GuardTarget),
    /// On the `prod` list: production-linked whatever else is true of it.
    Prod(GuardTarget),
    /// Not local and on no list: production-linked.
    Remote,
    /// A variable, or none named where the tool's default is remote: production-linked, because it is not known.
    Unresolved,
    /// A name that only describes the command (a bucket, a region); it never makes a command guarded unless the `prod` list names it.
    Info,
}

impl Class {
    fn production(&self) -> bool {
        matches!(self, Self::Prod(_) | Self::Remote | Self::Unresolved)
    }
}

/// One target of a command and what became of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Judged {
    pub kind: GuardTargetKind,
    /// The name as the command gives it (no credentials), or `unresolved` / `not named`.
    pub name: String,
    pub class: Class,
}

/// What the guard found in a command: its category and every target judged.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub category: GuardCategory,
    pub matched_by: String,
    pub targets: Vec<Judged>,
}

impl Found {
    /// Whether any target is production-linked.
    pub fn guarded(&self) -> bool {
        self.targets.iter().any(|t| t.class.production())
    }

    /// The targets that matter, for a line a person reads: the production-linked ones, else all.
    pub fn shown_target(&self) -> String {
        let pick: Vec<&Judged> = if self.guarded() {
            self.targets
                .iter()
                .filter(|t| t.class.production())
                .collect()
        } else {
            self.targets.iter().collect()
        };
        let text = pick
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if text.is_empty() {
            "local".to_owned()
        } else {
            text.chars().take(200).collect()
        }
    }

    /// The list entry that decided the verdict, when one did: a `prod` entry first, then a `safe` one.
    pub fn entry(&self) -> Option<&GuardTarget> {
        self.targets
            .iter()
            .find_map(|t| match &t.class {
                Class::Prod(entry) => Some(entry),
                _ => None,
            })
            .or_else(|| {
                self.targets.iter().find_map(|t| match &t.class {
                    Class::Safe(entry) => Some(entry),
                    _ => None,
                })
            })
    }
}

/// A command the guard holds: what it is, where it points and what matched.
#[derive(Clone, Debug, PartialEq)]
pub struct Blast {
    pub category: GuardCategory,
    pub target: String,
    pub matched_by: String,
    /// `prod` when the `prod` list forced it, else `none`.
    pub list: &'static str,
}

/// What the guard holds, if anything: the first destructive reading with a production-linked target.
pub fn blast(ctx: &Ctx<'_>, facts: &Facts) -> Option<Blast> {
    let found = inspect(ctx, facts)?;
    if !found.guarded() {
        return None;
    }
    Some(Blast {
        category: found.category,
        target: found.shown_target(),
        matched_by: found.matched_by.clone(),
        list: if found
            .targets
            .iter()
            .any(|t| matches!(t.class, Class::Prod(_)))
        {
            "prod"
        } else {
            "none"
        },
    })
}

/// Every destructive reading of the command, judged; the first guarded one, else the first. `None` is not destructive.
pub fn inspect(ctx: &Ctx<'_>, facts: &Facts) -> Option<Found> {
    if ctx.kind != orbyn_contracts::ActionKind::Command {
        return None;
    }
    let mut raws = Vec::new();
    for command in &ctx.commands {
        read(command, &ctx.target, None, &mut raws);
    }
    if raws.is_empty() {
        raws.extend(unparsed(ctx));
    }
    let mut found: Vec<Found> = raws.into_iter().map(|raw| judge(raw, facts)).collect();
    let at = found.iter().position(Found::guarded).unwrap_or(0);
    (!found.is_empty()).then(|| found.swap_remove(at))
}

// ---- reading -------------------------------------------------------------------------------------------------------------

/// A target as a command gives it, before it is judged.
#[derive(Clone, Debug)]
struct Raw {
    kind: GuardTargetKind,
    name: String,
    /// Known local (loopback, socket, path, the local daemon).
    local: bool,
    /// Only describes the command (a bucket, a region).
    info: bool,
    /// None was named where the tool's default is remote.
    missing: bool,
}

impl Raw {
    fn host(name: &str) -> Self {
        let (name, local) = host_of(name);
        Self {
            kind: GuardTargetKind::Host,
            name,
            local,
            info: false,
            missing: false,
        }
    }

    fn named(kind: GuardTargetKind, name: &str) -> Self {
        Self {
            kind,
            name: name.trim().to_lowercase(),
            local: false,
            info: false,
            missing: false,
        }
    }

    fn info(kind: GuardTargetKind, name: &str) -> Self {
        Self {
            info: true,
            ..Self::named(kind, name)
        }
    }

    fn missing(kind: GuardTargetKind) -> Self {
        Self {
            kind,
            name: "not named".to_owned(),
            local: false,
            info: false,
            missing: true,
        }
    }

    fn local() -> Self {
        Self {
            kind: GuardTargetKind::Host,
            name: "local".to_owned(),
            local: true,
            info: false,
            missing: false,
        }
    }
}

/// A destructive command read, with the targets it reaches.
struct Read {
    category: GuardCategory,
    matched_by: String,
    targets: Vec<Raw>,
}

fn variable(word: &str) -> bool {
    word.contains('$') || word.contains('`') || word.starts_with('%')
}

/// The host of a URL authority or a host word: lowercase, no port, no brackets. The second is whether it is local.
fn host_of(word: &str) -> (String, bool) {
    let word = word.trim().trim_matches(['\'', '"']);
    if word.is_empty() {
        return ("local socket".to_owned(), true);
    }
    if word.starts_with('/') || word.starts_with("unix:") || word.starts_with("npipe:") {
        return ("local socket".to_owned(), true);
    }
    let host = match word.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(rest).to_owned(),
        None => match word.rsplit_once(':') {
            Some((host, port))
                if port.chars().all(|c| c.is_ascii_digit()) && !host.contains(':') =>
            {
                host.to_owned()
            }
            _ => word.to_owned(),
        },
    };
    let host = host.to_lowercase();
    let local = host == "localhost"
        || host.ends_with(".localhost")
        || host.starts_with("127.")
        || host == "::1"
        || host == "0:0:0:0:0:0:0:1";
    (host, local)
}

fn url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)^([a-z][a-z0-9+.-]*)://(?:[^/@\s]*@)?(\[[0-9a-f:.]+\]|[^/:\s?#]*)(?::\d+)?"#,
        )
        .expect("a fixed pattern")
    })
}

/// The host of a URL, `None` for a word that is not one. An empty host (`postgres:///db`) is a local socket.
fn url_host(word: &str) -> Option<Raw> {
    let word = word.trim().trim_matches(['\'', '"']);
    let captures = url_re().captures(word)?;
    if captures[1].eq_ignore_ascii_case("file") {
        return Some(Raw::local());
    }
    Some(Raw::host(&captures[2]))
}

fn scp_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:[^@/:\s]+@)?(\[[0-9a-fA-F:.]+\]|[^@/:\s]+):").expect("a fixed pattern")
    })
}

/// The remote hosts a path-like word names: a URL, or `host:path` / `user@host:path` as rsync and git write them. A word that is
/// a path, a drive (`C:`) or an option is not a host. A variable host is kept, so it is read as unresolved.
fn remote_of(word: &str) -> Option<Raw> {
    if word.starts_with('-')
        || word.starts_with('/')
        || word.starts_with('.')
        || word.starts_with('~')
    {
        return None;
    }
    if word.contains("://") {
        return url_host(word);
    }
    let captures = scp_re().captures(word)?;
    let host = &captures[1];
    if host.len() == 1 && host.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(Raw::host(host))
}

fn env_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?:^|[\s;&|(])([A-Z][A-Z0-9_]*)=("(?:[^"\\]|\\.)*"|'[^']*'|[^\s;&|)]*)"#)
            .expect("a fixed pattern")
    })
}

/// The values the command text assigns to `names` in front of a program (`PGHOST=db psql ...`). The parser drops these words, so
/// they are read from the text.
fn env_values(text: &str, names: &[&str]) -> Vec<String> {
    env_re()
        .captures_iter(text)
        .filter(|c| names.contains(&&c[1]))
        .map(|c| c[2].trim_matches(['"', '\'']).to_owned())
        .collect()
}

/// Values of a flag written `--name value`, `--name=value` or, for a one-letter flag, `-nvalue`.
fn flag_values(args: &[String], names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < args.len() {
        let arg = args[at].as_str();
        for name in names {
            if arg == *name {
                if let Some(next) = args.get(at + 1) {
                    out.push(next.clone());
                }
            } else if let Some(rest) = arg.strip_prefix(&format!("{name}=")) {
                out.push(rest.to_owned());
            } else if name.len() == 2
                && !name.starts_with("--")
                && arg.starts_with(name)
                && arg.len() > 2
                && !arg.starts_with("--")
            {
                out.push(arg[2..].to_owned());
            }
        }
        at += 1;
    }
    out
}

/// The words that are not options, skipping the value of each option in `with_value`.
fn positional<'a>(args: &'a [String], with_value: &[&str]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
            continue;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            skip = with_value.contains(&arg.as_str());
            continue;
        }
        out.push(arg.as_str());
    }
    out
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// A short-option cluster (`-rf`) holding one of `letters`.
fn cluster(args: &[String], letters: &[char]) -> bool {
    args.iter().any(|a| {
        a.starts_with('-')
            && !a.starts_with("--")
            && a.chars().skip(1).any(|c| letters.contains(&c))
    })
}

/// Reads one simple command. `over` is the target of an `ssh` the command runs under: whatever the inner command reaches, it reaches
/// from that host.
fn read(command: &Simple, text: &str, over: Option<&Raw>, out: &mut Vec<Read>) {
    let args = &command.args;
    let mut reads = Vec::new();
    match command.program.as_str() {
        "ssh" => {
            ssh(command, text, out);
            return;
        }
        "rm" if cluster(args, &['r', 'R']) || has_flag(args, "--recursive") => {
            reads.push(files("rm -r", command, over));
        }
        "find" if has_flag(args, "-delete") => reads.push(files("find -delete", command, over)),
        "rsync" if args.iter().any(|a| a.starts_with("--delete")) => {
            reads.push(files("rsync --delete", command, over));
        }
        "shred" | "truncate" => reads.push(files(&command.program, command, over)),
        "psql" | "mysql" | "mariadb" | "mongosh" | "mongo" | "redis-cli" | "cqlsh" => {
            reads.extend(database(command, text, over));
        }
        "aws" => reads.extend(aws(command, text, over)),
        "gcloud" => reads.extend(gcloud(command, text, over)),
        "az" => reads.extend(az(command, text, over)),
        "gsutil" => reads.extend(gsutil(command, text, over)),
        "docker" => reads.extend(docker(command, text, over)),
        "git" => reads.extend(git(command, over)),
        _ => {}
    }
    out.extend(reads);
}

/// `ssh host cmd`: the inner command is read as if it ran on `host`, which is its target.
fn ssh(command: &Simple, text: &str, out: &mut Vec<Read>) {
    const WITH_VALUE: [&str; 17] = [
        "-p", "-i", "-l", "-o", "-F", "-J", "-b", "-c", "-D", "-E", "-e", "-I", "-L", "-m", "-O",
        "-R", "-S",
    ];
    let args = &command.args;
    let mut skip = false;
    let mut host_at = None;
    for (at, arg) in args.iter().enumerate() {
        if skip {
            skip = false;
            continue;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            skip = WITH_VALUE.contains(&arg.as_str());
            continue;
        }
        host_at = Some(at);
        break;
    }
    let Some(at) = host_at else { return };
    let word = args[at].rsplit('@').next().unwrap_or(&args[at]);
    let target = Raw::host(word);
    let rest = args[at + 1..].join(" ");
    if rest.trim().is_empty() {
        return;
    }
    for inner in simple_commands(&rest) {
        read(&inner, text, Some(&target), out);
    }
}

fn files(by: &str, command: &Simple, over: Option<&Raw>) -> Read {
    let mut targets = Vec::new();
    if let Some(over) = over {
        targets.push(over.clone());
    } else if command.program == "rsync" {
        targets.extend(command.args.iter().filter_map(|a| remote_of(a)));
    }
    if targets.is_empty() {
        targets.push(Raw::local());
    }
    Read {
        category: GuardCategory::FileDeletion,
        matched_by: by.to_owned(),
        targets,
    }
}

// ---- databases -----------------------------------------------------------------------------------------------------------

fn sql_patterns() -> &'static [(Regex, &'static str)] {
    static RE: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    RE.get_or_init(|| {
        [
            (r"(?i)\bdrop\s+\w", "DROP"),
            (r"(?i)\btruncate\b\s*(?:table\s+)?\S", "TRUNCATE"),
            (r"(?i)\bdelete\s+from\b", "DELETE"),
            (r"(?i)\bflush(?:all|db)\b", "FLUSHALL"),
            (r"(?i)\bdropdatabase\b", "dropDatabase"),
            (r"(?i)\.drop\s*\(", "drop()"),
            (r"(?i)\bdeletemany\s*\(", "deleteMany"),
            (r"(?i)\.remove\s*\(", "remove()"),
        ]
        .into_iter()
        .map(|(pattern, name)| (Regex::new(pattern).expect("a fixed pattern"), name))
        .collect()
    })
}

fn update_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bupdate\s+\S+\s+set\b").expect("a fixed pattern"))
}

/// The destructive word a query holds, if any: DROP, TRUNCATE, DELETE, FLUSHALL, dropDatabase, or an UPDATE with no WHERE.
fn destructive_sql(text: &str) -> Option<&'static str> {
    if let Some((_, name)) = sql_patterns().iter().find(|(re, _)| re.is_match(text)) {
        return Some(name);
    }
    text.split(';')
        .any(|statement| {
            update_re().is_match(statement)
                && !statement
                    .to_lowercase()
                    .split_whitespace()
                    .any(|w| w == "where")
        })
        .then_some("UPDATE without WHERE")
}

const DB_URL_ENV: [&str; 8] = [
    "PGHOST",
    "PGHOSTADDR",
    "MYSQL_HOST",
    "DATABASE_URL",
    "REDIS_URL",
    "MONGODB_URI",
    "MONGO_URL",
    "CASSANDRA_HOST",
];

fn database(command: &Simple, text: &str, over: Option<&Raw>) -> Option<Read> {
    let args = &command.args;
    // The query is in the arguments for `-c`, `-e`, `--eval` and redis-cli's own words; otherwise it is on the input.
    let inline_flags = ["-c", "--command", "-e", "--execute", "--eval"];
    let inline = args.iter().any(|a| {
        inline_flags.iter().any(|f| {
            a == f
                || a.starts_with(&format!("{f}="))
                || (f.len() == 2 && a.starts_with(f) && a.len() > 2 && !a.starts_with("--"))
        })
    }) || command.program == "redis-cli";
    let query = if inline {
        args.join(" ")
    } else {
        text.to_owned()
    };
    let word = destructive_sql(&query)?;
    let matched_by = format!("{}: {word}", command.program);
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Database,
            matched_by,
            targets: vec![over.clone()],
        });
    }
    let mut targets: Vec<Raw> = Vec::new();
    for arg in args {
        if let Some(raw) = url_host(arg) {
            targets.push(raw);
        } else if let Some(rest) = arg.find("host=").map(|at| &arg[at + 5..]) {
            targets.push(Raw::host(rest.split_whitespace().next().unwrap_or("")));
        }
    }
    for host in flag_values(args, &["-h", "--host", "--hostname"]) {
        targets.push(Raw::host(&host));
    }
    if !flag_values(args, &["-S", "--socket"]).is_empty() {
        targets.push(Raw::local());
    }
    for value in env_values(text, &DB_URL_ENV) {
        targets.push(url_host(&value).unwrap_or_else(|| Raw::host(&value)));
    }
    if command.program == "cqlsh"
        && let Some(host) = positional(
            args,
            &[
                "-u",
                "-p",
                "-k",
                "-e",
                "-f",
                "--username",
                "--password",
                "--keyspace",
                "--execute",
                "--file",
            ],
        )
        .first()
    {
        targets.push(Raw::host(host));
    }
    if command.program == "mongosh"
        && targets.is_empty()
        && let Some(word) = positional(
            args,
            &["--eval", "--port", "-u", "-p", "--username", "--password"],
        )
        .iter()
        .find(|w| w.contains('/') || w.contains(':'))
    {
        let head = word.split('/').next().unwrap_or(word);
        targets.push(Raw::host(head));
    }
    if targets.is_empty() {
        // The tool's default is whatever the environment says, which is not known here.
        targets.push(Raw::missing(GuardTargetKind::Host));
    }
    Some(Read {
        category: GuardCategory::Database,
        matched_by,
        targets,
    })
}

// ---- cloud ---------------------------------------------------------------------------------------------------------------

fn cloud_endpoint_local(args: &[String]) -> bool {
    flag_values(args, &["--endpoint-url", "--endpoint"])
        .iter()
        .any(|value| url_host(value).is_some_and(|raw| raw.local))
}

fn aws(command: &Simple, text: &str, over: Option<&Raw>) -> Option<Read> {
    let args = &command.args;
    let words = positional(
        args,
        &[
            "--profile",
            "--region",
            "--endpoint-url",
            "--output",
            "--query",
            "--cli-read-timeout",
            "--cli-connect-timeout",
            "--ca-bundle",
            "--color",
        ],
    );
    let (service, action) = (words.first().copied()?, words.get(1).copied().unwrap_or(""));
    let matched_by = match (service, action) {
        ("s3", "rm" | "rb") => format!("aws s3 {action}"),
        ("s3", "sync" | "cp" | "mv") if has_flag(args, "--delete") => {
            "aws s3 sync --delete".to_owned()
        }
        (_, a) if a.starts_with("delete-") || a.starts_with("terminate-") => {
            format!("aws {service} {a}")
        }
        _ => return None,
    };
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Cloud,
            matched_by,
            targets: vec![over.clone()],
        });
    }
    let mut targets: Vec<Raw> = Vec::new();
    let profiles: Vec<String> = flag_values(args, &["--profile"])
        .into_iter()
        .chain(env_values(text, &["AWS_PROFILE", "AWS_DEFAULT_PROFILE"]))
        .collect();
    targets.extend(
        profiles
            .iter()
            .map(|p| Raw::named(GuardTargetKind::Profile, p)),
    );
    for arg in args {
        if let Some(rest) = arg.strip_prefix("s3://") {
            targets.push(Raw::info(
                GuardTargetKind::Bucket,
                rest.split('/').next().unwrap_or(rest),
            ));
        }
    }
    for bucket in flag_values(args, &["--bucket"]) {
        targets.push(Raw::info(GuardTargetKind::Bucket, &bucket));
    }
    if cloud_endpoint_local(args) {
        for t in &mut targets {
            t.local = true;
        }
        if targets.is_empty() {
            targets.push(Raw::local());
        }
    } else if profiles.is_empty() {
        // No profile: the default one is used, which is not known here.
        targets.push(Raw::missing(GuardTargetKind::Profile));
    }
    Some(Read {
        category: GuardCategory::Cloud,
        matched_by,
        targets,
    })
}

fn gcloud(command: &Simple, text: &str, over: Option<&Raw>) -> Option<Read> {
    let args = &command.args;
    let words = positional(
        args,
        &[
            "--project",
            "--configuration",
            "--format",
            "--account",
            "--region",
            "--zone",
        ],
    );
    let destructive =
        words.contains(&"delete") || words.windows(2).any(|w| w[0] == "storage" && w[1] == "rm");
    if !destructive {
        return None;
    }
    let matched_by = if words.contains(&"delete") {
        "gcloud ... delete"
    } else {
        "gcloud storage rm"
    }
    .to_owned();
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Cloud,
            matched_by,
            targets: vec![over.clone()],
        });
    }
    let projects: Vec<String> = flag_values(args, &["--project"])
        .into_iter()
        .chain(env_values(
            text,
            &["CLOUDSDK_CORE_PROJECT", "GOOGLE_CLOUD_PROJECT"],
        ))
        .collect();
    let mut targets: Vec<Raw> = projects
        .iter()
        .map(|p| Raw::named(GuardTargetKind::Project, p))
        .collect();
    for arg in args {
        if let Some(rest) = arg.strip_prefix("gs://") {
            targets.push(Raw::info(
                GuardTargetKind::Bucket,
                rest.split('/').next().unwrap_or(rest),
            ));
        }
    }
    if projects.is_empty() {
        targets.push(Raw::missing(GuardTargetKind::Project));
    }
    Some(Read {
        category: GuardCategory::Cloud,
        matched_by,
        targets,
    })
}

fn az(command: &Simple, text: &str, over: Option<&Raw>) -> Option<Read> {
    let args = &command.args;
    if !args.iter().any(|a| a == "delete") {
        return None;
    }
    let matched_by = "az ... delete".to_owned();
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Cloud,
            matched_by,
            targets: vec![over.clone()],
        });
    }
    let subscriptions: Vec<String> = flag_values(args, &["--subscription"])
        .into_iter()
        .chain(env_values(text, &["AZURE_SUBSCRIPTION_ID"]))
        .collect();
    let mut targets: Vec<Raw> = subscriptions
        .iter()
        .map(|s| Raw::named(GuardTargetKind::Project, s))
        .collect();
    if subscriptions.is_empty() {
        targets.push(Raw::missing(GuardTargetKind::Project));
    }
    Some(Read {
        category: GuardCategory::Cloud,
        matched_by,
        targets,
    })
}

fn gsutil(command: &Simple, text: &str, over: Option<&Raw>) -> Option<Read> {
    let args = &command.args;
    let words = positional(args, &["-o", "-p", "-i"]);
    if words.first().copied() != Some("rm") {
        return None;
    }
    let matched_by = "gsutil rm".to_owned();
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Cloud,
            matched_by,
            targets: vec![over.clone()],
        });
    }
    let projects: Vec<String> = flag_values(args, &["-p"])
        .into_iter()
        .chain(env_values(
            text,
            &["CLOUDSDK_CORE_PROJECT", "GOOGLE_CLOUD_PROJECT"],
        ))
        .collect();
    let mut targets: Vec<Raw> = projects
        .iter()
        .map(|p| Raw::named(GuardTargetKind::Project, p))
        .collect();
    for arg in args {
        if let Some(rest) = arg.strip_prefix("gs://") {
            targets.push(Raw::info(
                GuardTargetKind::Bucket,
                rest.split('/').next().unwrap_or(rest),
            ));
        }
    }
    if projects.is_empty() {
        targets.push(Raw::missing(GuardTargetKind::Project));
    }
    Some(Read {
        category: GuardCategory::Cloud,
        matched_by,
        targets,
    })
}

// ---- containers ----------------------------------------------------------------------------------------------------------

fn docker(command: &Simple, text: &str, over: Option<&Raw>) -> Option<Read> {
    let args = &command.args;
    let words = positional(
        args,
        &[
            "-H",
            "--host",
            "--context",
            "-c",
            "--config",
            "-l",
            "--log-level",
            "--tlscacert",
            "--tlscert",
            "--tlskey",
        ],
    );
    let (first, second) = (words.first().copied()?, words.get(1).copied().unwrap_or(""));
    let matched_by = match (first, second) {
        ("rm" | "rmi" | "prune", _) => format!("docker {first}"),
        ("container" | "image" | "volume" | "network" | "system" | "builder", "rm" | "prune") => {
            format!("docker {first} {second}")
        }
        _ => return None,
    };
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Containers,
            matched_by,
            targets: vec![over.clone()],
        });
    }
    let mut targets: Vec<Raw> = Vec::new();
    for host in flag_values(args, &["-H", "--host"])
        .into_iter()
        .chain(env_values(text, &["DOCKER_HOST"]))
    {
        targets.push(url_host(&host).unwrap_or_else(|| Raw::host(&host)));
    }
    for context in flag_values(args, &["--context", "-c"])
        .into_iter()
        .chain(env_values(text, &["DOCKER_CONTEXT"]))
    {
        if context == "default" {
            targets.push(Raw::local());
        } else {
            targets.push(Raw::named(GuardTargetKind::Context, &context));
        }
    }
    if targets.is_empty() {
        // The local daemon: not remote, so not guarded.
        targets.push(Raw::local());
    }
    Some(Read {
        category: GuardCategory::Containers,
        matched_by,
        targets,
    })
}

// ---- git -----------------------------------------------------------------------------------------------------------------

/// The remotes are not known to the parser, so a push is read in two steps: here the push and its remote word, and in
/// [`judge`] the name is looked up. The remote word travels as a target whose kind is `Host` and whose name is `remote:<word>`.
fn git(command: &Simple, over: Option<&Raw>) -> Option<Read> {
    let args = &command.args;
    let mut at = 0;
    while at < args.len() {
        match args[at].as_str() {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" => at += 2,
            w if w.starts_with('-') => at += 1,
            _ => break,
        }
    }
    if args.get(at).map(String::as_str) != Some("push") {
        return None;
    }
    let rest = &args[at + 1..];
    let forced = rest.iter().any(|a| {
        a == "--force"
            || a.starts_with("--force-with-lease")
            || a == "--delete"
            || a == "--mirror"
            || a == "--prune"
    }) || cluster(rest, &['f', 'd']);
    let words = positional(
        rest,
        &["--repo", "-o", "--push-option", "--receive-pack", "--exec"],
    );
    let plus = words
        .iter()
        .skip(1)
        .any(|w| w.starts_with('+') || w.starts_with(':'));
    if !forced && !plus {
        return None;
    }
    let matched_by = if plus && !forced {
        "git push +refspec"
    } else if rest.iter().any(|a| a == "--delete") || cluster(rest, &['d']) {
        "git push --delete"
    } else {
        "git push --force"
    }
    .to_owned();
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Git,
            matched_by,
            targets: vec![over.clone()],
        });
    }
    let remote = flag_values(rest, &["--repo"])
        .into_iter()
        .next()
        .or_else(|| words.first().map(|w| (*w).to_owned()))
        .unwrap_or_else(|| "origin".to_owned());
    Some(Read {
        category: GuardCategory::Git,
        matched_by,
        targets: vec![Raw {
            kind: GuardTargetKind::Host,
            name: format!("remote:{remote}"),
            local: false,
            info: false,
            missing: false,
        }],
    })
}

// ---- the fallback --------------------------------------------------------------------------------------------------------

fn destructive_words() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\brm\s+-\w*[rR]|\bdrop\s+\w|\btruncate\b|\bdelete\s+from\b|\bdelete-\w|\bterminate-\w|\bflush(?:all|db)\b|--force\b|\bprune\b|--delete\b|\bshred\b")
            .expect("a fixed pattern")
    })
}

fn nonlocal_word() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b[a-z][a-z0-9+.-]*://[^\s'\x22]+|\b[\w.-]+@[\w.-]+|\b(?:[a-z0-9-]+\.)+[a-z]{2,24}\b|\b(?:\d{1,3}\.){3}\d{1,3}\b|\$\w+|\$\{")
            .expect("a fixed pattern")
    })
}

/// A command that holds a destructive word and a non-local word but that no category read: the parser found no command at all, a
/// quote is open, or the program is a variable. It is guarded, because the guard cannot tell what it reaches.
fn unparsed(ctx: &Ctx<'_>) -> Option<Read> {
    let text = ctx.target.as_str();
    let open_quote = text.matches('"').count() % 2 == 1 || text.matches('\'').count() % 2 == 1;
    let by_variable = ctx.commands.iter().any(|c| variable(&c.raw));
    if !(ctx.commands.is_empty() || open_quote || by_variable) {
        return None;
    }
    let destructive = destructive_words().find(text)?;
    let others = nonlocal_word().is_match(text);
    if !others {
        return None;
    }
    Some(Read {
        category: GuardCategory::Unparsed,
        matched_by: format!("unreadable: {}", destructive.as_str().trim()),
        targets: vec![Raw {
            kind: GuardTargetKind::Host,
            name: "unresolved".to_owned(),
            local: false,
            info: false,
            missing: true,
        }],
    })
}

// ---- judging -------------------------------------------------------------------------------------------------------------

/// Whether `pattern` (a name, or `*.suffix` for the suffix and what is under it) names `name`.
pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    let name = name.trim().trim_matches(['[', ']']).to_lowercase();
    let pattern = pattern.trim().to_lowercase();
    match pattern.strip_prefix("*.") {
        Some(suffix) => name == suffix || name.ends_with(&format!(".{suffix}")),
        None => name == pattern,
    }
}

fn listed(entries: &[GuardTarget], kind: GuardTargetKind, name: &str) -> Option<GuardTarget> {
    entries
        .iter()
        .find(|e| e.kind == kind && pattern_matches(&e.pattern, name))
        .cloned()
}

fn judge(read: Read, facts: &Facts) -> Found {
    let targets = read
        .targets
        .into_iter()
        .map(|raw| judge_one(raw, facts))
        .collect();
    Found {
        category: read.category,
        matched_by: read.matched_by,
        targets,
    }
}

fn judge_one(mut raw: Raw, facts: &Facts) -> Judged {
    // A git remote travels as its word and is resolved here, from the worktree's own remotes.
    if let Some(word) = raw.name.strip_prefix("remote:").map(str::to_owned) {
        raw = resolve_remote(&word, &facts.remotes);
    }
    let judged = |class| Judged {
        kind: raw.kind,
        name: raw.name.clone(),
        class,
    };
    if raw.missing || variable(&raw.name) {
        return judged(Class::Unresolved);
    }
    if let Some(entry) = listed(&facts.prod, raw.kind, &raw.name) {
        return judged(Class::Prod(entry));
    }
    if raw.local {
        return judged(Class::Local);
    }
    if raw.info {
        return judged(Class::Info);
    }
    match listed(&facts.safe, raw.kind, &raw.name) {
        Some(entry) => judged(Class::Safe(entry)),
        None => judged(Class::Remote),
    }
}

/// A git remote word as the target it points at: a URL or path given directly, or a name looked up in the worktree's remotes. A
/// name that is not there is unresolved.
fn resolve_remote(word: &str, remotes: &HashMap<String, String>) -> Raw {
    let direct = word.contains('/') || word.contains(':') || word.ends_with(".git");
    let url = if direct {
        Some(word.to_owned())
    } else {
        remotes.get(word).cloned()
    };
    let Some(url) = url else {
        return Raw::missing(GuardTargetKind::Host);
    };
    let url = url.trim();
    if url.starts_with('/')
        || url.starts_with("./")
        || url.starts_with("../")
        || url.starts_with("file://")
        || url
            .as_bytes()
            .get(1..3)
            .is_some_and(|b| b == b":\\" || b == b":/")
    {
        return Raw::local();
    }
    remote_of(url).unwrap_or_else(|| {
        // A bare relative path (`../mirror.git`, `mirror`) is a local path; anything else that is not a URL is not known.
        if url.contains(':') {
            Raw::missing(GuardTargetKind::Host)
        } else {
            Raw::local()
        }
    })
}

/// Reads `git remote get-url --push` for each remote of a worktree, hardened as the other reads of a worker's repository are. A
/// remote that cannot be read is left out, which the guard reads as unresolved.
pub(super) async fn read_remotes(worktree: &Path) -> HashMap<String, String> {
    use crate::source_control::git::{git_hardened, text};
    let mut found = HashMap::new();
    let Ok(listed) = git_hardened(worktree, &["remote"]).await else {
        return found;
    };
    for name in text(&listed).lines().take(20) {
        let name = name.trim();
        if name.is_empty() || name.starts_with('-') || name.len() > 100 {
            continue;
        }
        if let Ok(url) = git_hardened(worktree, &["remote", "get-url", "--push", name]).await {
            found.insert(name.to_owned(), text(&url));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use orbyn_contracts::ActionKind;

    use super::super::hard::{Scene, context};
    use super::*;

    fn entry(list: GuardList, kind: GuardTargetKind, pattern: &str) -> GuardTarget {
        GuardTarget {
            id: format!("gt-{pattern}"),
            list,
            pattern: pattern.to_owned(),
            kind,
            note: None,
            added_at: "2026-01-01T00:00:00Z".to_owned(),
        }
    }

    fn remotes() -> HashMap<String, String> {
        [
            ("origin", "git@github.com:orbyn-dev/app.git"),
            ("mirror", "/srv/git/app.git"),
            ("self", "file:///srv/git/app.git"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
    }

    fn facts(entries: Vec<GuardTarget>) -> Facts {
        Facts::new(entries, remotes())
    }

    fn look(command: &str, facts: &Facts) -> Option<Found> {
        let scene = Scene {
            worktree: Path::new("/work/tree"),
            protected: &[],
            allowed_hosts: &[],
            data_dir: None,
            branch: None,
            default_branch: None,
        };
        let ctx = context(ActionKind::Command, command, &[], &scene);
        inspect(&ctx, facts)
    }

    fn guarded(command: &str) -> bool {
        look(command, &facts(Vec::new())).is_some_and(|f| f.guarded())
    }

    fn category(command: &str) -> Option<GuardCategory> {
        look(command, &facts(Vec::new())).map(|f| f.category)
    }

    #[test]
    fn each_category_is_found_in_a_command_for_a_remote_target() {
        use GuardCategory::*;
        let table = [
            ("ssh prod.example.com 'rm -rf /srv/data'", FileDeletion),
            ("ssh deploy@box rm -rf /srv/data", FileDeletion),
            (
                "rsync -a --delete ./out/ deploy@prod.example.com:/srv/www/",
                FileDeletion,
            ),
            (
                "ssh prod.example.com 'find /srv -name x -delete'",
                FileDeletion,
            ),
            ("ssh prod.example.com shred /srv/key", FileDeletion),
            (
                "ssh prod.example.com truncate -s 0 /var/log/x",
                FileDeletion,
            ),
            (
                "psql -h db.prod.example.com -c 'DROP TABLE users'",
                Database,
            ),
            ("mysql -h db.example.com -e 'TRUNCATE users'", Database),
            ("mysql -h db.example.com -e 'DELETE FROM users'", Database),
            (
                "psql postgres://u:pw@db.example.com/app -c 'UPDATE users SET x = 1'",
                Database,
            ),
            ("redis-cli -h cache.example.com FLUSHALL", Database),
            (
                "mongosh mongodb://db.example.com/app --eval 'db.dropDatabase()'",
                Database,
            ),
            ("cqlsh db.example.com -e 'DROP KEYSPACE app'", Database),
            ("aws s3 rm s3://logs --recursive --profile prod", Cloud),
            ("aws s3 rb s3://logs --profile prod", Cloud),
            (
                "aws ec2 terminate-instances --instance-ids i-1 --profile prod",
                Cloud,
            ),
            ("aws rds delete-db-instance --profile prod", Cloud),
            ("gcloud compute instances delete vm --project acme", Cloud),
            ("az group delete --name rg --subscription sub-1", Cloud),
            ("gsutil -m rm -r gs://bucket/x", Cloud),
            ("docker -H tcp://10.0.0.5:2376 rm -f web", Containers),
            ("docker --context prod system prune -af", Containers),
            ("git push --force origin feature", Git),
            ("git push origin --delete feature", Git),
            ("git push origin +feature", Git),
            ("git push -f origin feature", Git),
        ];
        for (command, want) in table {
            assert_eq!(category(command), Some(want), "{command}");
            assert!(guarded(command), "{command}");
        }
    }

    #[test]
    fn a_command_that_is_not_destructive_is_not_read() {
        for command in [
            "ls -la",
            "rm file.txt",
            "psql -h db.example.com -c 'SELECT 1'",
            "psql -h db.example.com -c 'UPDATE t SET x = 1 WHERE id = 3'",
            "aws s3 ls s3://logs --profile prod",
            "git push origin feature",
            "docker ps",
            "docker -H tcp://10.0.0.5:2376 ps",
            "cargo test",
        ] {
            assert_eq!(look(command, &facts(Vec::new())), None, "{command}");
        }
    }

    #[test]
    fn a_variable_or_a_missing_target_is_guarded() {
        for command in [
            "psql -h $DB_HOST -c 'DROP TABLE x'",
            "psql -c 'DROP TABLE x'",
            "redis-cli FLUSHALL",
            "mysql -e 'DELETE FROM t'",
            "aws s3 rm s3://bucket --recursive",
            "aws ec2 terminate-instances --instance-ids i-1",
            "gcloud compute instances delete vm",
            "ssh $TARGET 'rm -rf /srv'",
            "ssh \"$TARGET\" rm -rf /srv",
            "git push --force",
            "git push --force nowhere",
            "docker --context $CTX rm x",
            "docker -H $HOST rm x",
        ] {
            assert!(guarded(command), "{command}");
        }
        let found = look("psql -c 'DROP TABLE x'", &facts(Vec::new())).unwrap();
        assert_eq!(found.targets[0].class, Class::Unresolved);
        assert_eq!(found.targets[0].name, "not named");
    }

    #[test]
    fn targets_come_from_env_prefixes_urls_and_flags() {
        let f = |c: &str| look(c, &facts(Vec::new())).unwrap();
        let found = f("PGHOST=db.example.com psql -c 'DROP TABLE x'");
        assert_eq!(found.targets[0].name, "db.example.com");
        let found =
            f("DATABASE_URL=postgres://u:pw@db.example.com:5432/app psql -c 'DROP TABLE x'");
        assert_eq!(found.targets[0].name, "db.example.com");
        assert!(!found.shown_target().contains("pw"));
        let found = f("AWS_PROFILE=prod aws s3 rb s3://x");
        assert!(
            found
                .targets
                .iter()
                .any(|t| t.name == "prod" && t.kind == GuardTargetKind::Profile)
        );
        let found = f("docker --context staging rm x");
        assert_eq!(found.targets[0].kind, GuardTargetKind::Context);
        let found = f("gcloud compute instances delete vm --project=acme-prod");
        assert!(found.targets.iter().any(|t| t.name == "acme-prod"));
        let found = f("git push -f origin main");
        assert_eq!(found.targets[0].name, "github.com");
    }

    #[test]
    fn local_targets_are_not_guarded() {
        for command in [
            "psql -h localhost -c 'DROP TABLE x'",
            "psql -h 127.0.0.1 -c 'DROP TABLE x'",
            "psql -h ::1 -c 'DROP TABLE x'",
            "psql -h /var/run/postgresql -c 'DROP TABLE x'",
            "psql postgres:///app -c 'DROP TABLE x'",
            "PGHOST=localhost psql -c 'DROP TABLE x'",
            "mysql -S /tmp/mysql.sock -e 'DROP TABLE x'",
            "redis-cli -h 127.0.0.1 FLUSHALL",
            "rm -rf build",
            "rm -rf /work/tree/target",
            "find . -name '*.tmp' -delete",
            "rsync -a --delete src/ dest/",
            "docker rm web",
            "docker system prune -af",
            "docker --context default rm web",
            "docker -H unix:///var/run/docker.sock rm web",
            "docker -H tcp://127.0.0.1:2375 rm web",
            "git push --force mirror feature",
            "git push --force self feature",
            "git push --force /srv/git/app.git feature",
            "git push --force ../mirror.git feature",
            "aws s3 rm s3://b --recursive --endpoint-url http://localhost:4566",
        ] {
            let found = look(command, &facts(Vec::new()));
            assert!(
                found.is_some_and(|f| !f.guarded()),
                "{command} should be read, and local"
            );
        }
    }

    #[test]
    fn the_safe_list_exempts_and_the_prod_list_forces_and_prod_wins() {
        use GuardList::*;
        use GuardTargetKind::*;
        let safe = facts(vec![entry(Safe, Host, "*.staging.example.com")]);
        let command = "psql -h db.staging.example.com -c 'DROP TABLE x'";
        let found = look(command, &safe).unwrap();
        assert!(!found.guarded());
        assert!(matches!(found.targets[0].class, Class::Safe(_)));
        assert_eq!(
            found.entry().map(|e| e.pattern.as_str()),
            Some("*.staging.example.com")
        );
        // The exemption is for the listed host only.
        assert!(
            look("psql -h db.example.com -c 'DROP TABLE x'", &safe)
                .unwrap()
                .guarded()
        );
        assert!(
            look(
                "psql -h staging.example.com.evil.io -c 'DROP TABLE x'",
                &safe
            )
            .unwrap()
            .guarded()
        );
        // A kind must match: a profile entry does not exempt a host.
        let wrong = facts(vec![entry(Safe, Profile, "db.staging.example.com")]);
        assert!(look(command, &wrong).unwrap().guarded());
        // The prod list forces, even for a name that looks local.
        let forced = facts(vec![entry(Prod, Host, "localhost")]);
        assert!(
            look("psql -h localhost -c 'DROP TABLE x'", &forced)
                .unwrap()
                .guarded()
        );
        // Prod wins when a target is in both.
        let both = facts(vec![
            entry(Safe, Host, "db.staging.example.com"),
            entry(Prod, Host, "db.staging.example.com"),
        ]);
        let found = look(command, &both).unwrap();
        assert!(found.guarded());
        assert!(matches!(found.targets[0].class, Class::Prod(_)));
        let held = {
            let scene = Scene {
                worktree: Path::new("/work/tree"),
                protected: &[],
                allowed_hosts: &[],
                data_dir: None,
                branch: None,
                default_branch: None,
            };
            blast(&context(ActionKind::Command, command, &[], &scene), &both).unwrap()
        };
        assert_eq!(held.list, "prod");
    }

    #[test]
    fn a_safe_profile_does_not_exempt_a_command_with_another_unlisted_target() {
        use GuardList::*;
        use GuardTargetKind::*;
        let safe = facts(vec![entry(Safe, Profile, "dev")]);
        assert!(
            !look("aws s3 rm s3://any --profile dev", &safe)
                .unwrap()
                .guarded()
        );
        // A bucket only describes the command, but a prod bucket forces it.
        let both = facts(vec![
            entry(Safe, Profile, "dev"),
            entry(Prod, Bucket, "customer-data"),
        ]);
        assert!(
            look("aws s3 rm s3://customer-data --profile dev", &both)
                .unwrap()
                .guarded()
        );
        // Two profiles, one safe: guarded.
        assert!(
            look("AWS_PROFILE=prod aws s3 rm s3://b --profile dev", &safe)
                .unwrap()
                .guarded()
        );
    }

    #[test]
    fn an_unreadable_command_with_a_destructive_and_a_non_local_word_is_guarded() {
        assert_eq!(
            category("echo 'oops; rm -rf /srv user@prod.example.com"),
            Some(GuardCategory::Unparsed)
        );
        assert!(guarded("$CMD psql -h db.example.com DROP TABLE x"));
        // No non-local word, or no destructive word: not read.
        assert_eq!(look("echo 'oops; rm -rf build", &facts(Vec::new())), None);
        assert_eq!(
            look("echo 'oops prod.example.com", &facts(Vec::new())),
            None
        );
    }

    #[test]
    fn a_sql_file_is_not_read_and_the_limit_is_documented() {
        // Known limit: what `-f` runs is not read.
        assert_eq!(
            look("psql -h db.example.com -f drop.sql", &facts(Vec::new())),
            None
        );
    }

    #[test]
    fn stdin_queries_are_read() {
        assert!(guarded("echo 'DROP TABLE users' | psql -h db.example.com"));
        assert!(guarded(
            "psql -h db.example.com <<EOF\nTRUNCATE users;\nEOF"
        ));
    }

    #[test]
    fn a_pattern_is_exact_or_a_suffix_and_never_a_look_alike() {
        assert!(pattern_matches("db.example.com", "DB.example.com"));
        assert!(pattern_matches("*.example.com", "a.b.example.com"));
        assert!(pattern_matches("*.example.com", "example.com"));
        assert!(!pattern_matches("*.example.com", "notexample.com"));
        assert!(!pattern_matches("*.example.com", "example.com.evil.io"));
        assert!(!pattern_matches("db.example.com", "xdb.example.com"));
    }

    #[test]
    fn only_a_command_is_read() {
        let scene = Scene {
            worktree: Path::new("/work/tree"),
            protected: &[],
            allowed_hosts: &[],
            data_dir: None,
            branch: None,
            default_branch: None,
        };
        let ctx = context(ActionKind::Network, "psql -c 'DROP TABLE x'", &[], &scene);
        assert_eq!(inspect(&ctx, &facts(Vec::new())), None);
    }
}
