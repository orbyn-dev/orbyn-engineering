//! Reading a command well enough to refuse what the always-denied list names. This is not a
//! shell: it splits a command into simple commands (on `;`, `&&`, `||`, `|`, `&`, newlines,
//! parentheses and backticks), unwraps `sudo`, `env`, `sh -c` and `eval`, drops quotes, and finds
//! the words that are paths, hosts and redirect targets. What it cannot see is in the README:
//! a script that does the forbidden thing inside itself, an obfuscated or encoded command, an
//! alias or a function. Where a word is a variable or `~`, it is read as outside the worktree.

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

/// One command with its words and the targets of its redirects, quotes removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Simple {
    pub program: String,
    /// The first word as written, so `./run.sh` is still a path.
    pub raw: String,
    pub args: Vec<String>,
    pub redirects: Vec<String>,
}

const WRAPPERS: [&str; 18] = [
    "env",
    "command",
    "nohup",
    "time",
    "nice",
    "exec",
    "timeout",
    "stdbuf",
    "xargs",
    "caffeinate",
    "setsid",
    "chroot",
    "unshare",
    "flock",
    "watch",
    "parallel",
    "ionice",
    "aws-vault",
];
const SHELLS: [&str; 9] = [
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "fish",
    "cmd",
    "powershell",
    "pwsh",
];

/// The simple commands in `text`, wrappers undone and nested command text parsed too.
pub fn simple_commands(text: &str) -> Vec<Simple> {
    let mut out = Vec::new();
    parse(text, 0, &mut out);
    out
}

#[allow(unused_assignments)]
fn parse(text: &str, depth: u8, out: &mut Vec<Simple>) {
    if depth > 4 {
        return;
    }
    let mut words: Vec<String> = Vec::new();
    let mut redirects: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut redirect = false;
    let mut chars = text.chars().peekable();
    macro_rules! end_word {
        () => {
            if started {
                if redirect {
                    redirect = false;
                    if !word.starts_with('&') {
                        redirects.push(std::mem::take(&mut word));
                    }
                } else {
                    words.push(std::mem::take(&mut word));
                }
                word.clear();
                started = false;
            }
        };
    }
    macro_rules! end_command {
        () => {
            end_word!();
            redirect = false;
            finish(
                std::mem::take(&mut words),
                std::mem::take(&mut redirects),
                depth,
                out,
            );
        };
    }
    while let Some(c) = chars.next() {
        if let Some(open) = quote {
            if c == open {
                quote = None;
            } else if c == '\\' && open == '"' {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            } else {
                word.push(c);
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                started = true;
            }
            '\\' => {
                // A backslash at the end of a line joins it to the next: both characters go (agents often wrap long commands).
                if chars.peek() == Some(&'\r') {
                    chars.next();
                }
                if chars.peek() == Some(&'\n') {
                    chars.next();
                } else if let Some(next) = chars.next() {
                    word.push(next);
                    started = true;
                }
            }
            c if c.is_whitespace() && c != '\n' => {
                end_word!();
            }
            '&' if chars.peek() == Some(&'>') => {}
            '(' | ')' if started && !word.ends_with('$') => word.push(c),
            ';' | '\n' | '|' | '&' | '(' | ')' | '`' => {
                end_command!();
            }
            '>' | '<' => {
                // A bare file descriptor in front of it (`2>`) is not a word.
                if started && word.chars().all(|d| d.is_ascii_digit()) {
                    word.clear();
                    started = false;
                } else {
                    end_word!();
                }
                if chars.peek() == Some(&c) {
                    chars.next();
                }
                if c == '>' {
                    redirect = true;
                }
                // `2>&1`: the `&1` is a descriptor, not a file.
                if chars.peek() == Some(&'&') {
                    chars.next();
                    word.push('&');
                    started = true;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    end_command!();
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// The name a program goes by: its last path part, lowercase, with no `.exe`.
pub fn program_name(word: &str) -> String {
    let base = word
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(word)
        .to_lowercase();
    base.strip_suffix(".exe")
        .map_or(base.clone(), str::to_owned)
}

/// The options of a wrapper that take a value as a separate word (`sudo -u root`, `xargs -I {}`, `env -u NAME`): the value is not the
/// program.
fn value_options(wrapper: &str) -> &'static [&'static str] {
    match wrapper {
        "sudo" => &[
            "-u",
            "-g",
            "-h",
            "-p",
            "-C",
            "-T",
            "-r",
            "-t",
            "-U",
            "-D",
            "-R",
            "--user",
            "--group",
            "--host",
            "--prompt",
            "--close-from",
            "--command-timeout",
            "--role",
            "--type",
            "--other-user",
            "--chdir",
            "--chroot",
        ],
        "doas" => &["-u", "-C"],
        "env" => &["-u", "-C", "-S", "--unset", "--chdir", "--split-string"],
        "xargs" => &[
            "-I",
            "-n",
            "-P",
            "-L",
            "-s",
            "-d",
            "-E",
            "-a",
            "-J",
            "-R",
            "-S",
            "--max-args",
            "--max-procs",
            "--max-lines",
            "--max-chars",
            "--delimiter",
            "--arg-file",
            "--replace",
            "--eof",
        ],
        "nice" => &["-n", "--adjustment"],
        "timeout" => &["-s", "-k", "--signal", "--kill-after"],
        "stdbuf" => &["-i", "-o", "-e"],
        "time" => &["-f", "-o", "--format", "--output"],
        "exec" => &["-a"],
        "caffeinate" => &["-t", "-w"],
        "flock" => &["-w", "-E", "--timeout", "--conflict-exit-code"],
        "watch" => &["-n", "--interval"],
        "parallel" => &[
            "-j",
            "-n",
            "-N",
            "-S",
            "-a",
            "--jobs",
            "--max-args",
            "--sshlogin",
        ],
        "ionice" => &["-c", "-n", "-p", "--class", "--classdata"],
        _ => &[],
    }
}

/// Drops a wrapper's options, their values, `--`, assignments and a bare number or duration (`timeout 5s`), leaving its command.
fn skip_options(words: &mut Vec<String>, wrapper: &str) -> Option<String> {
    let values = value_options(wrapper);
    let mut split = None;
    while let Some(word) = words.first().cloned() {
        if word == "--" {
            words.remove(0);
            break;
        }
        if wrapper == "env" {
            // `env -S'cmd'` and `env --split-string='cmd'`: the value is attached.
            let attached = word.strip_prefix("--split-string=").or_else(|| {
                word.strip_prefix("-S")
                    .filter(|v| !v.is_empty() && !word.starts_with("--"))
            });
            if let Some(value) = attached {
                split = Some(value.to_owned());
                words.remove(0);
                continue;
            }
        }
        if word.starts_with('-') && word.len() > 1 {
            words.remove(0);
            if values.contains(&word.as_str()) && !words.is_empty() {
                let value = words.remove(0);
                // `env -S 'cmd args'` runs the string as a command line.
                if wrapper == "env" && matches!(word.as_str(), "-S" | "--split-string") {
                    split = Some(value);
                }
            }
        } else if is_assignment(&word)
            || word
                .trim_end_matches(['s', 'm', 'h', 'd'])
                .parse::<f64>()
                .is_ok()
        {
            words.remove(0);
        } else {
            break;
        }
    }
    // These take a positional word before the command: the lock file, the new root. `aws-vault exec <profile>` likewise.
    if matches!(wrapper, "flock" | "chroot") && !words.is_empty() && split.is_none() {
        words.remove(0);
    }
    if wrapper == "flock"
        && split.is_none()
        && matches!(words.first().map(String::as_str), Some("-c" | "--command"))
        && words.len() > 1
    {
        split = Some(words[1].clone());
        words.clear();
    }
    if wrapper == "aws-vault" {
        if let Some(at) = words.iter().position(|w| w == "--") {
            words.drain(..=at);
        } else if let Some(at) = words.iter().position(|w| w == "exec") {
            // No `--`: the word after `exec` and its options is the profile, and what follows is the command.
            words.drain(..=at);
            while words.first().is_some_and(|w| w.starts_with('-')) {
                let option = words.remove(0);
                if matches!(
                    option.as_str(),
                    "--duration"
                        | "-d"
                        | "--region"
                        | "--mfa-token"
                        | "-t"
                        | "--prompt"
                        | "--backend"
                ) && !words.is_empty()
                {
                    words.remove(0);
                }
            }
            if !words.is_empty() {
                words.remove(0);
            }
        } else {
            words.clear();
        }
    }
    split
}

fn finish(mut words: Vec<String>, redirects: Vec<String>, depth: u8, out: &mut Vec<Simple>) {
    for word in &words {
        nested(word, depth, out);
    }
    loop {
        while words.first().is_some_and(|w| is_assignment(w)) {
            words.remove(0);
        }
        let Some(first) = words.first() else { return };
        let name = program_name(first);
        if name == "sudo" || name == "doas" || name == "su" {
            // A privilege change is refused on its own; the command it wraps is read too.
            out.push(Simple {
                program: name.clone(),
                raw: words[0].clone(),
                args: words[1..].to_vec(),
                redirects: Vec::new(),
            });
            words.remove(0);
            skip_options(&mut words, &name);
            continue;
        }
        if WRAPPERS.contains(&name.as_str()) {
            words.remove(0);
            if let Some(split) = skip_options(&mut words, &name) {
                let line = format!("{split} {}", words.join(" "));
                parse(&line, depth + 1, out);
                return;
            }
            if words.is_empty() {
                return;
            }
            continue;
        }
        break;
    }
    let name = program_name(&words[0]);
    let args = words[1..].to_vec();
    if name == "eval" {
        parse(&args.join(" "), depth + 1, out);
        return;
    }
    if (SHELLS.contains(&name.as_str()) || name == "script")
        && let Some(at) = args.iter().position(|a| {
            let a = a.to_lowercase();
            matches!(
                a.as_str(),
                "-c" | "-lc" | "-ic" | "/c" | "/k" | "-command" | "-c:" | "--command"
            )
        })
        && let Some(inner) = args.get(at + 1)
    {
        parse(inner, depth + 1, out);
    }
    out.push(Simple {
        program: name,
        raw: words[0].clone(),
        args,
        redirects,
    });
}

/// Command substitution inside a word: `$(...)` and backticks.
fn nested(word: &str, depth: u8, out: &mut Vec<Simple>) {
    if let Some(at) = word.find("$(") {
        let rest = &word[at + 2..];
        let end = rest.rfind(')').unwrap_or(rest.len());
        parse(&rest[..end], depth + 1, out);
    }
    if word.matches('`').count() >= 2
        && let (Some(a), Some(b)) = (word.find('`'), word.rfind('`'))
        && b > a
    {
        parse(&word[a + 1..b], depth + 1, out);
    }
}

fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Whether a word is written as a path: absolute, `~`, a variable, `..`, a drive.
pub fn is_path_like(word: &str) -> bool {
    word.starts_with(['/', '~', '$', '%', '\\'])
        || word.starts_with("..")
        || word.contains("../")
        || word.contains("..\\")
        || word
            .as_bytes()
            .get(1..3)
            .is_some_and(|rest| rest == b":\\" || rest == b":/")
}

/// Whether `word`, read as a path from `worktree`, lands outside it. A variable or `~` is read as
/// outside, because it is not known; `/dev/null` is not a place.
pub fn outside(word: &str, worktree: &Path) -> bool {
    outside_from(word, Some(worktree), worktree)
}

fn is_absolute_word(word: &str) -> bool {
    Path::new(word).is_absolute()
        || word.starts_with(['/', '\\'])
        || is_path_like(word) && word.as_bytes().get(1) == Some(&b':')
}

fn roots(worktree: &Path) -> Vec<PathBuf> {
    let mut roots = vec![lexical(worktree)];
    if let Ok(real) = crate::util::canonicalize(worktree) {
        roots.push(real);
    }
    roots
}

/// As [`outside`], for a command that runs in `cwd` after a `cd`. `None` is a directory that is not known, so a relative
/// word is read as outside.
pub fn outside_from(word: &str, cwd: Option<&Path>, worktree: &Path) -> bool {
    if matches!(word, "/dev/null" | "NUL" | "nul") {
        return false;
    }
    if word.starts_with(['~', '$', '%']) {
        return true;
    }
    let joined = if is_absolute_word(word) {
        lexical(Path::new(word))
    } else {
        match cwd {
            Some(cwd) => lexical(&cwd.join(word)),
            None => return true,
        }
    };
    !roots(worktree).iter().any(|root| joined.starts_with(root))
}

/// The directory after `cd word` from `cwd`: `None` when it is not known (no argument, a variable, `-`, an unknown start).
pub fn change_dir(cwd: Option<&Path>, word: Option<&str>) -> Option<PathBuf> {
    let word = word?;
    if word.starts_with(['~', '$', '%', '-']) {
        return None;
    }
    if is_absolute_word(word) {
        return Some(lexical(Path::new(word)));
    }
    Some(lexical(&cwd?.join(word)))
}

/// A repository-relative form of `word` (forward slashes, no `./`), when it lies inside `worktree`.
pub fn relative(word: &str, worktree: &Path) -> Option<String> {
    relative_from(word, Some(worktree), worktree)
}

pub fn relative_from(word: &str, cwd: Option<&Path>, worktree: &Path) -> Option<String> {
    if outside_from(word, cwd, worktree) {
        return None;
    }
    let full = if is_absolute_word(word) {
        lexical(Path::new(word))
    } else {
        lexical(&cwd?.join(word))
    };
    roots(worktree).iter().find_map(|root| {
        full.strip_prefix(root).ok().map(|rest| {
            rest.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
        })
    })
}

fn url_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)\b[a-z][a-z0-9+.-]*://(?:[^/@\s'"<>]*@)?(\[[0-9a-f:.]+\]|[^/:\s'"<>?#]+)"#,
        )
        .expect("a fixed pattern")
    })
}

fn host_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:[\w.~-]+@)?((?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,24}|(?:\d{1,3}\.){3}\d{1,3})(?::\d+)?(?:[/?#].*)?$")
            .expect("a fixed pattern")
    })
}

fn scp_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?:[\w.~-]+@)?((?:[A-Za-z0-9-]+\.)+[A-Za-z]{2,24}|(?:\d{1,3}\.){3}\d{1,3}):[^/]",
        )
        .expect("a fixed pattern")
    })
}

/// The programs that open a connection to a host named as an argument.
const CONNECTORS: [&str; 14] = [
    "curl", "wget", "http", "https", "xh", "nc", "ncat", "netcat", "telnet", "ftp", "ping", "ssh",
    "sftp", "mosh",
];

/// Every host a command reaches for: URL hosts anywhere, `host:path` forms for git, ssh, scp and
/// rsync, and the host argument of a connecting program.
pub fn hosts(command: &Simple) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let text = std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    for capture in url_pattern().captures_iter(&text) {
        found.push(capture[1].to_lowercase());
    }
    let scp_like = matches!(
        command.program.as_str(),
        "git" | "ssh" | "scp" | "rsync" | "sftp"
    );
    let connector = CONNECTORS.contains(&command.program.as_str());
    for arg in &command.args {
        if arg.starts_with('-') || arg.contains("://") {
            continue;
        }
        if scp_like && let Some(capture) = scp_pattern().captures(arg) {
            found.push(capture[1].to_lowercase());
        } else if connector && let Some(capture) = host_pattern().captures(arg) {
            found.push(capture[1].to_lowercase());
        } else if matches!(command.program.as_str(), "ssh" | "sftp" | "mosh")
            && let Some((_, host)) = arg.split_once('@')
        {
            found.push(host.split(':').next().unwrap_or(host).to_lowercase());
        }
    }
    // `ssh -o HostName=prod` and `-o ProxyJump=prod` name hosts that are not a leading word.
    if matches!(
        command.program.as_str(),
        "ssh" | "scp" | "sftp" | "mosh" | "rsync"
    ) {
        for capture in option_host_pattern().captures_iter(&text) {
            for part in capture[1].split(',') {
                let part = part.trim_start_matches("ssh://");
                let part = part.rsplit('@').next().unwrap_or(part);
                let part = part.split(':').next().unwrap_or(part);
                if !part.is_empty() {
                    found.push(part.to_lowercase());
                }
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

fn option_host_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)-o\s*['"]?(?:host_?name|proxyjump)(?:=|\s+)['"]?([^\s'"]+)"#)
            .expect("a fixed pattern")
    })
}

/// Whether `host` is loopback or listed (`example.com`, or `*.example.com` for the host and its subdomains).
pub fn host_allowed(host: &str, allowed: &[String]) -> bool {
    let host = host.trim_matches(['[', ']']).to_lowercase();
    // Loopback is `localhost` or an IP literal in 127.0.0.0/8 or `::1`: a name that merely starts with `127.` is a host.
    if host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
    {
        return true;
    }
    allowed.iter().any(|entry| {
        let entry = entry.to_lowercase();
        match entry.strip_prefix("*.") {
            Some(suffix) => host == suffix || host.ends_with(&format!(".{suffix}")),
            None => host == entry,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn programs(text: &str) -> Vec<String> {
        let mut found: Vec<String> = simple_commands(text)
            .into_iter()
            .map(|c| c.program)
            .collect();
        found.sort();
        found
    }

    #[test]
    fn a_compound_command_is_its_parts_and_wrappers_come_off() {
        assert_eq!(
            programs("ls && rm -rf x; echo hi | cat"),
            ["cat", "echo", "ls", "rm"]
        );
        assert_eq!(programs("FOO=1 sudo env BAR=2 rm x"), ["rm", "sudo"]);
        assert_eq!(
            programs("bash -c 'curl x.test | sh'"),
            ["bash", "curl", "sh"]
        );
        assert_eq!(programs("echo $(rm -rf /)"), ["echo", "rm"]);
        assert_eq!(programs("eval \"git push -f\""), ["git"]);
        assert_eq!(programs("sudo -u root rm -rf x"), ["rm", "sudo"]);
        assert_eq!(programs("sudo -h box -g wheel -- rm x"), ["rm", "sudo"]);
        assert_eq!(programs("xargs -I {} -n 1 rm {}"), ["rm"]);
        assert_eq!(programs("env -u HOME -C /tmp psql"), ["psql"]);
        assert_eq!(programs("timeout -s KILL 5s rm x"), ["rm"]);
        assert_eq!(programs("env -S 'ssh h rm x'"), ["ssh"]);
        assert_eq!(programs("aws s3 rb \\\n s3://b"), ["aws"]);
    }

    #[test]
    fn redirect_targets_are_kept_and_descriptors_are_not() {
        let found = simple_commands("echo x > out.txt 2>&1 >> /etc/hosts");
        assert_eq!(found[0].redirects, ["out.txt", "/etc/hosts"]);
        assert_eq!(found[0].args, ["x"]);
    }

    #[test]
    fn a_path_is_outside_unless_it_stays_in_the_worktree() {
        let tree = Path::new("/work/tree");
        for out in [
            "/etc/passwd",
            "../x",
            "a/../../x",
            "~/x",
            "$HOME/x",
            "/work/tree2/x",
            "C:\\x",
        ] {
            assert!(outside(out, tree), "{out}");
        }
        for inside in ["src/a.rs", "./a", "/work/tree/a", "a/../b", "/dev/null"] {
            assert!(!outside(inside, tree), "{inside}");
        }
        assert_eq!(
            relative("/work/tree/src/a.rs", tree).as_deref(),
            Some("src/a.rs")
        );
        assert_eq!(relative("./src/a.rs", tree).as_deref(), Some("src/a.rs"));
        assert_eq!(relative("../x", tree), None);
    }

    #[test]
    fn hosts_are_found_in_urls_scp_forms_and_connector_arguments() {
        let host = |text: &str| hosts(&simple_commands(text)[0]);
        assert_eq!(
            host("curl -s https://user@Evil.example/x?y=1"),
            ["evil.example"]
        );
        assert_eq!(host("git clone git@github.com:o/r.git"), ["github.com"]);
        assert_eq!(host("ssh dev@box.example.org ls"), ["box.example.org"]);
        assert_eq!(host("curl example.com/path"), ["example.com"]);
        assert_eq!(host("wget http://[::1]:8080/x"), ["[::1]"]);
        assert!(host("rsync -a file.txt dir/").is_empty());
        assert!(host("git push origin feature").is_empty());
        assert_eq!(
            host("ssh -o HostName=prod.example.com localhost ls"),
            ["prod.example.com"]
        );
        assert_eq!(
            host("ssh -o ProxyJump=ops@jump.example.com localhost ls"),
            ["jump.example.com"]
        );
        // The remote word `hostname` is not an option.
        assert_eq!(
            host("ssh allowed.example.com hostname -f"),
            ["allowed.example.com"]
        );
    }

    #[test]
    fn allowed_hosts_are_exact_or_a_wildcard_and_loopback_is_always_fine() {
        let allowed = vec!["github.com".to_owned(), "*.example.org".to_owned()];
        for ok in [
            "github.com",
            "GITHUB.com",
            "a.example.org",
            "example.org",
            "localhost",
            "127.0.0.1",
            "[::1]",
        ] {
            assert!(host_allowed(ok, &allowed), "{ok}");
        }
        for bad in [
            "evil.com",
            "github.com.evil.com",
            "notexample.org",
            "0.0.0.0",
            "127.evil.com",
            "127.0.0.1.evil.com",
            "localhost.evil.com",
        ] {
            assert!(!host_allowed(bad, &allowed), "{bad}");
        }
    }
}
