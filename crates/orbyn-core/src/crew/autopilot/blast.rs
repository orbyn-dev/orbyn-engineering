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

use self::sql::destructive_sql;
use self::ssh::ssh;
use super::hard::Ctx;
use super::shell::{Simple, simple_commands};

/// What the guard needs besides the command: this device's two lists and the worktree's git remotes (name to every push URL).
#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub prod: Vec<GuardTarget>,
    pub safe: Vec<GuardTarget>,
    pub remotes: HashMap<String, Vec<String>>,
}

impl Facts {
    /// Splits the stored entries into the two lists.
    pub fn new(entries: Vec<GuardTarget>, remotes: HashMap<String, Vec<String>>) -> Self {
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
    /// A hash of what it resolved from (a git remote's URL), when it resolved from something.
    pub ident: Option<String>,
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

    /// Every target as `kind:name` (with a hash of what it resolved from, if any), sorted: what an approval is bound to, so a target
    /// that changes after the command was held (a `git remote set-url`) is a different command.
    pub fn resolved(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .targets
            .iter()
            .map(|t| match &t.ident {
                Some(ident) => format!("{:?}:{}#{ident}", t.kind, t.name),
                None => format!("{:?}:{}", t.kind, t.name),
            })
            .collect();
        out.sort();
        out.dedup();
        out
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
    /// [`Found::resolved`]: the targets an approval is bound to.
    pub resolved: Vec<String>,
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
        resolved: found.resolved(),
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
    /// What the target resolved from, hashed (a git remote's URL), so a change of it is a change of target.
    ident: Option<String>,
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
            ident: None,
        }
    }

    fn named(kind: GuardTargetKind, name: &str) -> Self {
        Self {
            kind,
            name: name.trim().to_lowercase(),
            local: false,
            info: false,
            missing: false,
            ident: None,
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
            ident: None,
        }
    }

    fn local() -> Self {
        Self {
            kind: GuardTargetKind::Host,
            name: "local".to_owned(),
            local: true,
            info: false,
            missing: false,
            ident: None,
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

/// Whether one host word is local: `localhost`, or an IP literal that is a loopback address (127.0.0.0/8, `::1`). Nothing else is, so
/// `127.evil.com`, `127.0.0.1.evil.com` and `0.0.0.0` are not.
fn is_loopback(host: &str) -> bool {
    let host = host.trim().trim_matches(['[', ']']);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// One `host[:port]` word: lowercase, no port, no brackets. The second is whether it is local. An empty word and a path are a unix
/// socket.
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
    let local = is_loopback(&host);
    (host, local)
}

/// A comma-separated host list (`psql -h a,b`, a URL's authority): one target each. A list is local only if every host is.
fn hosts(word: &str) -> Vec<Raw> {
    let word = word.trim().trim_matches(['\'', '"']);
    word.split(',').map(Raw::host).collect()
}

/// `%2F` and friends in a URL query value, enough to read a socket directory.
fn unescape(value: &str) -> String {
    value
        .replace("%2F", "/")
        .replace("%2f", "/")
        .replace("%3A", ":")
        .replace("%3a", ":")
}

/// The hosts of a URL (`postgresql://u:p@127.0.0.1:1,db.prod/app?host=x`), `None` for a word that is not one. An empty authority is a
/// local socket unless the query names `host=`/`hostaddr=`, which then decide.
fn url_hosts(word: &str) -> Option<Vec<Raw>> {
    let word = word.trim().trim_matches(['\'', '"']);
    let (scheme, rest) = word.split_once("://")?;
    let valid = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    if !valid {
        return None;
    }
    if scheme.eq_ignore_ascii_case("file") {
        return Some(vec![Raw::local()]);
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let tail = &rest[end..];
    let host_part = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let mut out: Vec<Raw> = if host_part.is_empty() {
        Vec::new()
    } else {
        hosts(host_part)
    };
    if let Some((_, query)) = tail.split_once('?') {
        for pair in query.split(['&', '#']) {
            if let Some(value) = pair
                .strip_prefix("host=")
                .or_else(|| pair.strip_prefix("hostaddr="))
            {
                out.extend(hosts(&unescape(value)));
            } else if pair.starts_with("service=") {
                out.push(Raw::missing(GuardTargetKind::Host));
            }
        }
    }
    if out.is_empty() {
        out.push(Raw::local());
    }
    Some(out)
}

/// The hosts a libpq connection string (`host=a,b dbname=x`) names; `service=` names none that can be read, so it is unknown.
fn conninfo_hosts(text: &str) -> Vec<Raw> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        match key {
            "host" | "hostaddr" => out.extend(hosts(value)),
            "service" => out.push(Raw::missing(GuardTargetKind::Host)),
            _ => {}
        }
    }
    out
}

fn scp_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:[^@/:\s]+@)?(\[[0-9a-fA-F:.]+\]|[^@/:\s]+):").expect("a fixed pattern")
    })
}

/// The remote hosts a path-like word names: a URL, or `host:path` / `user@host:path` as rsync and git write them. A word that is
/// a path, a drive (`C:`) or an option is not a host. A variable host is kept, so it is read as unresolved.
fn remote_hosts(word: &str) -> Vec<Raw> {
    if word.starts_with('-')
        || word.starts_with('/')
        || word.starts_with('.')
        || word.starts_with('~')
    {
        return Vec::new();
    }
    if word.contains("://") {
        return url_hosts(word).unwrap_or_default();
    }
    let Some(captures) = scp_re().captures(word) else {
        return Vec::new();
    };
    let host = &captures[1];
    if host.len() == 1 && host.chars().all(|c| c.is_ascii_alphabetic()) {
        return Vec::new();
    }
    vec![Raw::host(host)]
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

/// The names the command text assigns in front of a program.
fn env_names(text: &str) -> Vec<String> {
    env_re()
        .captures_iter(text)
        .map(|c| c[1].to_owned())
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
fn read(command: &Simple, text: &str, over: Option<&[Raw]>, out: &mut Vec<Read>) {
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
        "find" if find_destroys(args) => reads.push(files("find -delete", command, over)),
        "rsync"
            if args.iter().any(|a| {
                a.starts_with("--delete") || a == "--del" || a == "--remove-source-files"
            }) =>
        {
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
        "git" => reads.extend(git(command, text, over)),
        "docker-compose" => {
            let mut compose = command.clone();
            compose.args.insert(0, "compose".to_owned());
            reads.extend(docker(&compose, text, over));
        }
        other if !KNOWN_TOOLS.contains(&other) && !HARMLESS.contains(&other) => {
            suffix_scan(command, text, over, out);
        }
        _ => {}
    }
    out.extend(reads);
}

/// Programs the guard reads itself.
const KNOWN_TOOLS: [&str; 18] = [
    "ssh",
    "rm",
    "find",
    "rsync",
    "shred",
    "truncate",
    "psql",
    "mysql",
    "mariadb",
    "mongosh",
    "mongo",
    "redis-cli",
    "cqlsh",
    "aws",
    "gcloud",
    "az",
    "gsutil",
    "docker",
];

/// Programs that only print or search their arguments: a destructive tool named in them is text, not a command.
const HARMLESS: [&str; 19] = [
    "echo", "printf", "cat", "grep", "rg", "egrep", "fgrep", "ls", "head", "tail", "less", "man",
    "which", "type", "sed", "awk", "sudo", "doas", "su",
];

/// A program the guard does not know, followed by the name of a tool it does: a wrapper the parser did not unwrap (`unknown-tool
/// --flag rm -rf x`). Each later word that names a tool is read as the start of a command, so the wrapper cannot hide it.
fn suffix_scan(command: &Simple, text: &str, over: Option<&[Raw]>, out: &mut Vec<Read>) {
    for at in 0..command.args.len().min(16) {
        let word = &command.args[at];
        if word.starts_with('-') {
            continue;
        }
        let name = super::shell::program_name(word);
        if KNOWN_TOOLS.contains(&name.as_str()) || name == "git" || name == "docker-compose" {
            let inner = Simple {
                program: name,
                raw: word.clone(),
                args: command.args[at + 1..].to_vec(),
                redirects: Vec::new(),
            };
            read(&inner, text, over, out);
        }
    }
}

/// The programs a `find -exec` runs that remove files.
fn find_destroys(args: &[String]) -> bool {
    if has_flag(args, "-delete") {
        return true;
    }
    let Some(at) = args
        .iter()
        .position(|a| matches!(a.as_str(), "-exec" | "-execdir" | "-ok" | "-okdir"))
    else {
        return false;
    };
    args[at + 1..].iter().any(|a| {
        matches!(
            super::shell::program_name(a).as_str(),
            "rm" | "shred" | "unlink" | "truncate"
        )
    })
}

fn files(by: &str, command: &Simple, over: Option<&[Raw]>) -> Read {
    let mut targets = Vec::new();
    if let Some(over) = over {
        targets.extend_from_slice(over);
    } else if command.program == "rsync" {
        targets.extend(command.args.iter().flat_map(|a| remote_hosts(a)));
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

/// The queries a client is given on its command line.
fn inline_queries(command: &Simple) -> Vec<String> {
    if command.program == "redis-cli" {
        return vec![command.args.join(" ")];
    }
    flag_values(
        &command.args,
        &["-c", "--command", "-e", "--execute", "--eval"],
    )
}

/// A word that may be a URL or a connection string, read for the hosts it names.
fn db_hosts_of(word: &str, out: &mut Vec<Raw>) {
    if let Some(found) = url_hosts(word) {
        out.extend(found);
    } else if word.contains('=') {
        out.extend(conninfo_hosts(word));
    }
}

fn database(command: &Simple, text: &str, over: Option<&[Raw]>) -> Option<Read> {
    let args = &command.args;
    let queries = inline_queries(command);
    // A pipe, a here-doc, a redirect or `-f -` can feed the client SQL as well, even beside `-c`.
    let stdin = text.contains(['|', '<', '\n'])
        || args
            .windows(2)
            .any(|w| matches!(w[0].as_str(), "-f" | "--file") && w[1] == "-")
        || args.iter().any(|a| a == "--file=-");
    let mysql = matches!(command.program.as_str(), "mysql" | "mariadb");
    // What feeds the client, without the host words (a host named `drop.example.com` is not SQL).
    let mut fed_text = text.to_owned();
    for host in flag_values(args, &["-h", "--host", "--hostname"]) {
        fed_text = fed_text.replace(&host, " ");
    }
    for arg in args {
        if !arg.starts_with('-') && !queries.iter().any(|q| q == arg) {
            let mut named = Vec::new();
            db_hosts_of(arg, &mut named);
            if !named.is_empty() {
                fed_text = fed_text.replace(arg.as_str(), " ");
            }
        }
    }
    let word = queries
        .iter()
        .find_map(|q| destructive_sql(q, false, mysql))
        .or_else(|| {
            if stdin {
                destructive_sql(&fed_text, true, mysql)
            } else {
                None
            }
        })?;
    let matched_by = format!("{}: {word}", command.program);
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Database,
            matched_by,
            targets: over.to_vec(),
        });
    }
    let mut targets: Vec<Raw> = Vec::new();
    for arg in args {
        if arg.starts_with('-') || queries.iter().any(|q| q == arg) {
            continue;
        }
        db_hosts_of(arg, &mut targets);
    }
    for value in flag_values(args, &["-d", "--dbname", "-u", "--uri", "--url"]) {
        db_hosts_of(&value, &mut targets);
    }
    for host in flag_values(args, &["-h", "--host", "--hostname"]) {
        targets.extend(hosts(&host));
    }
    if !flag_values(args, &["-S", "--socket"]).is_empty() {
        targets.push(Raw::local());
    }
    for value in env_values(text, &DB_URL_ENV) {
        if let Some(found) = url_hosts(&value) {
            targets.extend(found);
        } else {
            targets.extend(hosts(&value));
        }
    }
    // A service name hides the host in a file this reads nothing of.
    if !env_values(text, &["PGSERVICE"]).is_empty() {
        targets.push(Raw::missing(GuardTargetKind::Host));
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
        targets.extend(hosts(host));
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
        targets.extend(hosts(head));
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
        .any(|value| url_hosts(value).is_some_and(|all| all.iter().all(|raw| raw.local)))
}

fn aws(command: &Simple, text: &str, over: Option<&[Raw]>) -> Option<Read> {
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
            "--cli-binary-format",
            "--ca-bundle",
            "--color",
            "--endpoint",
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
            targets: over.to_vec(),
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
    // Static keys, a session token or an endpoint in the environment override whatever profile is named: the account is unknown.
    if env_names(text).iter().any(|name| {
        matches!(
            name.as_str(),
            "AWS_ACCESS_KEY_ID"
                | "AWS_SECRET_ACCESS_KEY"
                | "AWS_SESSION_TOKEN"
                | "AWS_CONFIG_FILE"
                | "AWS_SHARED_CREDENTIALS_FILE"
        ) || name.starts_with("AWS_ENDPOINT_URL")
    }) {
        targets.push(Raw::missing(GuardTargetKind::Profile));
    }
    Some(Read {
        category: GuardCategory::Cloud,
        matched_by,
        targets,
    })
}

fn gcloud(command: &Simple, text: &str, over: Option<&[Raw]>) -> Option<Read> {
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
            targets: over.to_vec(),
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

fn az(command: &Simple, text: &str, over: Option<&[Raw]>) -> Option<Read> {
    let args = &command.args;
    if !args
        .iter()
        .any(|a| a == "delete" || a.starts_with("delete-"))
    {
        return None;
    }
    let matched_by = "az ... delete".to_owned();
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Cloud,
            matched_by,
            targets: over.to_vec(),
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

fn gsutil(command: &Simple, text: &str, over: Option<&[Raw]>) -> Option<Read> {
    let args = &command.args;
    let words = positional(args, &["-o", "-p", "-i"]);
    let matched_by = match words.first().copied() {
        Some("rm") => "gsutil rm".to_owned(),
        Some("rb") => "gsutil rb".to_owned(),
        Some("rsync") if cluster(args, &['d']) => "gsutil rsync -d".to_owned(),
        _ => return None,
    };
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Cloud,
            matched_by,
            targets: over.to_vec(),
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

fn docker(command: &Simple, text: &str, over: Option<&[Raw]>) -> Option<Read> {
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
            "--file",
            "-f",
            "-p",
            "--project-name",
            "--env-file",
            "--profile",
            "--project-directory",
        ],
    );
    let (first, second) = (words.first().copied()?, words.get(1).copied().unwrap_or(""));
    // `docker compose down -v|--volumes|--rmi` and `compose rm` remove containers, volumes or images.
    let compose_destroys = first == "compose"
        && (second == "rm"
            || second == "down"
                && (cluster(args, &['v'])
                    || has_flag(args, "--volumes")
                    || args.iter().any(|a| a.starts_with("--rmi"))));
    let matched_by = match (first, second) {
        ("compose", _) if compose_destroys => format!("docker compose {second}"),
        ("rm" | "rmi" | "prune", _) => format!("docker {first}"),
        (
            "container" | "image" | "volume" | "network" | "system" | "builder" | "stack"
            | "service" | "secret" | "config" | "node" | "plugin",
            "rm" | "prune",
        ) => {
            format!("docker {first} {second}")
        }
        _ => return None,
    };
    if let Some(over) = over {
        return Some(Read {
            category: GuardCategory::Containers,
            matched_by,
            targets: over.to_vec(),
        });
    }
    let mut targets: Vec<Raw> = Vec::new();
    for host in flag_values(args, &["-H", "--host"])
        .into_iter()
        .chain(env_values(text, &["DOCKER_HOST"]))
    {
        match url_hosts(&host) {
            Some(found) => targets.extend(found),
            None => targets.extend(hosts(&host)),
        }
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

fn git_env_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:^|[\s;&|(])GIT_(?:CONFIG\w*|DIR|WORK_TREE|COMMON_DIR)=")
            .expect("a fixed pattern")
    })
}

fn git_remote_change_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\bgit\b[^;&|\n]*\b(?:remote\s+(?:set-url|add|rename|remove|rm|set-branches)\b|config\b[^;&|\n]*\b(?:remote|url)\.)")
            .expect("a fixed pattern")
    })
}

/// The remotes are not known to the parser, so a push is read in two steps: here the push and its remote word, and in
/// [`judge`] the name is looked up (every push URL of it). The remote word travels as a target whose kind is `Host` and whose name is
/// `remote:<word>`. A push that could be redirected by the command itself (`-c remote.x.pushurl=...`, `-C`, `GIT_CONFIG_*` and
/// `GIT_DIR` in the environment) adds a target that cannot be resolved, so it is guarded.
fn git(command: &Simple, text: &str, over: Option<&[Raw]>) -> Option<Read> {
    let args = &command.args;
    let mut at = 0;
    // A `git remote set-url|add|rename|remove` or `git config remote.*` earlier in the same command line changes what the push goes to
    // before it runs, so the remotes read now are not the ones it will use.
    let mut redirected = git_env_re().is_match(text) || git_remote_change_re().is_match(text);
    while at < args.len() {
        let word = args[at].as_str();
        match word {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--config-env" => {
                redirected |= word != "--namespace";
                at += 2;
            }
            w if w.starts_with("--git-dir=")
                || w.starts_with("--work-tree=")
                || w.starts_with("--config-env=")
                || (w.starts_with("-c") && w.len() > 2 && !w.starts_with("--"))
                || (w.starts_with("-C") && w.len() > 2 && !w.starts_with("--")) =>
            {
                redirected = true;
                at += 1;
            }
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
    let is_refspec = |w: &&str| w.starts_with('+') || w.starts_with(':');
    let remote_word = words.first().filter(|w| !is_refspec(w)).copied();
    let plus = words.iter().any(is_refspec);
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
            targets: over.to_vec(),
        });
    }
    let remote = flag_values(rest, &["--repo"])
        .into_iter()
        .next()
        .or_else(|| remote_word.map(str::to_owned))
        .unwrap_or_else(|| "origin".to_owned());
    let mut targets = vec![Raw {
        kind: GuardTargetKind::Host,
        name: format!("remote:{remote}"),
        local: false,
        info: false,
        missing: false,
        ident: None,
    }];
    if redirected {
        targets.push(Raw::missing(GuardTargetKind::Host));
    }
    Some(Read {
        category: GuardCategory::Git,
        matched_by,
        targets,
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
            ident: None,
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
        .flat_map(|raw| judge_one(raw, facts))
        .collect();
    Found {
        category: read.category,
        matched_by: read.matched_by,
        targets,
    }
}

fn judge_one(raw: Raw, facts: &Facts) -> Vec<Judged> {
    // A git remote travels as its word and is resolved here, from the worktree's own remotes: every push URL of it.
    if let Some(word) = raw.name.strip_prefix("remote:") {
        return resolve_remote(word, &facts.remotes)
            .into_iter()
            .map(|r| judge_plain(r, facts))
            .collect();
    }
    vec![judge_plain(raw, facts)]
}

fn judge_plain(raw: Raw, facts: &Facts) -> Judged {
    let judged = |class| Judged {
        kind: raw.kind,
        name: raw.name.clone(),
        class,
        ident: raw.ident.clone(),
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

/// A git remote word as the targets it pushes to: a URL or path given directly, or every push URL of a name in the worktree's
/// remotes. A name that is not there is unresolved.
fn resolve_remote(word: &str, remotes: &HashMap<String, Vec<String>>) -> Vec<Raw> {
    let direct = word.contains('/') || word.contains(':') || word.ends_with(".git");
    let urls: Vec<String> = if direct {
        vec![word.to_owned()]
    } else {
        remotes.get(word).cloned().unwrap_or_default()
    };
    if urls.is_empty() {
        return vec![Raw::missing(GuardTargetKind::Host)];
    }
    let mut out = Vec::new();
    for url in urls {
        let url = url.trim();
        let ident = Some(crate::util::sha256_hex(url.as_bytes())[..16].to_owned());
        let mut found = if url.starts_with('/')
            || url.starts_with("./")
            || url.starts_with("../")
            || url.starts_with("file://")
            || url
                .as_bytes()
                .get(1..3)
                .is_some_and(|b| b == b":\\" || b == b":/")
        {
            vec![Raw::local()]
        } else {
            let hosts = remote_hosts(url);
            if !hosts.is_empty() {
                hosts
            } else if url.contains(':') {
                // Not a URL and not a path: not known.
                vec![Raw::missing(GuardTargetKind::Host)]
            } else {
                // A bare relative path (`../mirror.git`, `mirror`) is a local path.
                vec![Raw::local()]
            }
        };
        for raw in &mut found {
            raw.ident = ident.clone();
        }
        out.extend(found);
    }
    out
}

/// Reads every push URL of each remote of a worktree (`git remote get-url --push --all`, which applies `pushurl`, `insteadOf` and
/// `pushInsteadOf`), hardened as the other reads of a worker's repository are. A remote that cannot be read is left out, which the
/// guard reads as unresolved.
pub(super) async fn read_remotes(worktree: &Path) -> HashMap<String, Vec<String>> {
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
        if let Ok(urls) =
            git_hardened(worktree, &["remote", "get-url", "--push", "--all", name]).await
        {
            let urls: Vec<String> = text(&urls)
                .lines()
                .map(|l| l.trim().to_owned())
                .filter(|l| !l.is_empty())
                .take(10)
                .collect();
            found.insert(name.to_owned(), urls);
        }
    }
    found
}

mod sql;
mod ssh;
#[cfg(test)]
mod tests;
