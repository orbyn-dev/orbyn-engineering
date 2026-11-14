//! Reading `ssh`: its options, every host they name, the remote command, and the text fed to a remote shell.

use super::*;

/// OpenSSH's options that take a value.
const SSH_WITH_VALUE: &str = "BbcDEeFIiJLlmOoPpQRSWw";

/// What `ssh`'s arguments hold: the host word, and every option that takes a value, as `(letter, value)`.
struct SshArgs {
    host_at: usize,
    options: Vec<(char, String)>,
}

/// The host word of `ssh`'s arguments (after the options and their values, and after `--`) and the options that take a value.
fn ssh_parse(args: &[String]) -> Option<SshArgs> {
    let mut at = 0;
    let mut options = Vec::new();
    while at < args.len() {
        let arg = args[at].as_str();
        if arg == "--" {
            return (at + 1 < args.len()).then_some(SshArgs {
                host_at: at + 1,
                options,
            });
        }
        if !arg.starts_with('-') || arg.len() == 1 {
            return Some(SshArgs {
                host_at: at,
                options,
            });
        }
        // A cluster such as `-vp 22` or `-p22`: the first option that takes a value ends it.
        let letters: Vec<char> = arg.chars().skip(1).collect();
        if let Some(i) = letters.iter().position(|c| SSH_WITH_VALUE.contains(*c)) {
            if i + 1 == letters.len() {
                if let Some(value) = args.get(at + 1) {
                    options.push((letters[i], value.clone()));
                }
                at += 2;
            } else {
                options.push((letters[i], letters[i + 1..].iter().collect()));
                at += 1;
            }
        } else {
            at += 1;
        }
    }
    None
}

/// A jump host as written (`user@host:port`, `ssh://host`), as a target.
fn jump_hosts(list: &str) -> Vec<Raw> {
    list.split(',')
        .map(|j| {
            let j = j.trim().trim_start_matches("ssh://");
            Raw::host(j.rsplit('@').next().unwrap_or(j))
        })
        .collect()
}

fn heredoc_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"<<-?\s*['"]?(\w+)"#).expect("a fixed pattern"))
}

fn input_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?:^|[^<])<\s*([^\s<>|;&]+)").expect("a fixed pattern"))
}

const INTERPRETERS: [&str; 10] = [
    "sh", "bash", "zsh", "dash", "ksh", "fish", "python", "python3", "perl", "ruby",
];

/// `ssh host cmd`: the inner command is read as if it ran on `host`, which is its target, together with every jump host, `HostName`
/// and forward the command line names. A proxy or local command, a config file, a `RemoteCommand` nothing was read from, and a
/// remote command (or a script fed to a shell by a pipe or here-doc) that holds a destructive word no category read are guarded as
/// unreadable, unless every target is local or safe.
pub(super) fn ssh(command: &Simple, text: &str, out: &mut Vec<Read>) {
    let args = &command.args;
    let Some(parsed) = ssh_parse(args) else {
        return;
    };
    let at = parsed.host_at;
    let word = args[at].rsplit('@').next().unwrap_or(&args[at]);
    let mut targets = vec![Raw::host(word)];
    let mut remote_option: Option<String> = None;
    let mut always = false;
    for (letter, value) in &parsed.options {
        match letter {
            'J' => targets.extend(jump_hosts(value)),
            'F' => {
                targets.push(Raw::missing(GuardTargetKind::Host));
            }
            'L' | 'R' => {
                // [bind:]port:host:hostport
                let parts: Vec<&str> = value.split(':').collect();
                if parts.len() >= 3 {
                    targets.push(Raw::host(parts[parts.len() - 2]));
                }
            }
            'W' => {
                if let Some((host, _)) = value.rsplit_once(':') {
                    targets.push(Raw::host(host));
                }
            }
            'o' => {
                let split = value.find(['=', ' ']);
                let (key, val) = match split {
                    Some(i) => (&value[..i], value[i + 1..].trim().trim_matches('"')),
                    None => (value.as_str(), ""),
                };
                match key.to_lowercase().as_str() {
                    "hostname" => targets.extend(hosts(val)),
                    "proxyjump" => targets.extend(jump_hosts(val)),
                    "remotecommand" => remote_option = Some(val.to_owned()),
                    "proxycommand" | "localcommand" => {
                        targets.push(Raw::missing(GuardTargetKind::Host));
                        always = true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    // The delimiter of a here-doc is parsed as a word of the command; it is not part of the remote command.
    let delimiter = heredoc_re()
        .captures(text)
        .map(|c| c[1].to_owned())
        .unwrap_or_default();
    // So is the file of a plain `< file` redirect.
    let input_file = input_re()
        .captures(text)
        .map(|c| c[1].to_owned())
        .unwrap_or_default();
    let mut rest = args[at + 1..]
        .iter()
        .filter(|w| delimiter.is_empty() || **w != delimiter)
        .filter(|w| input_file.is_empty() || **w != input_file)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if rest.trim().is_empty()
        && let Some(option) = &remote_option
    {
        rest.clone_from(option);
    }
    let fed = text.contains('<') || piped_into_ssh(text);
    let before = out.len();
    if !rest.trim().is_empty() {
        for inner in simple_commands(&rest) {
            read(&inner, text, Some(&targets), out);
        }
    }
    if out.len() != before {
        return;
    }
    // Every command of the remote line counts, wrappers (`sudo`, `env`, ...) already taken off: `sudo bash`, `sudo -i`, `su -` are shells.
    let inner = simple_commands(&rest);
    let interpreter = rest.trim().is_empty()
        || inner.iter().any(|c| {
            INTERPRETERS.contains(&c.program.as_str())
                || c.program == "su"
                || matches!(c.program.as_str(), "sudo" | "doas")
                    && c.args.iter().any(|a| {
                        a.starts_with('-') && !a.starts_with("--") && a.contains(['s', 'i'])
                            || matches!(a.as_str(), "--shell" | "--login")
                    })
        });
    let why = if always {
        Some("ssh: a proxy or local command")
    } else if remote_option.is_some() {
        Some("ssh: a RemoteCommand that could not be read")
    } else if !rest.trim().is_empty() && (destructive_words().is_match(&rest) || variable(&rest)) {
        Some("ssh: a remote command that could not be read")
    } else if fed && interpreter && (destructive_words().is_match(text) || producer_unknown(text)) {
        Some("ssh: a script fed to the remote shell")
    } else {
        None
    };
    if let Some(why) = why {
        out.push(Read {
            category: GuardCategory::Unparsed,
            matched_by: why.to_owned(),
            targets,
        });
    }
}

/// Whether something is piped INTO `ssh`: `ssh` is not the first stage of a pipeline. `ssh -G host | grep x` only reads its output.
fn piped_into_ssh(text: &str) -> bool {
    let text = text.replace("||", ";");
    let stages: Vec<&str> = text.split('|').collect();
    stages.iter().position(|stage| {
        stage
            .split_whitespace()
            .any(|w| w == "ssh" || w.ends_with("/ssh"))
    }) > Some(0)
}

/// Whether the text that feeds `ssh` comes from something other than a literal (`echo`, `printf`): a file or a program's output is
/// not read here.
fn producer_unknown(text: &str) -> bool {
    // `< file` feeds a file this does not read; `<< EOF` puts the text in the command, which is scanned.
    (text.contains('<') && !text.contains("<<"))
        || simple_commands(text)
            .iter()
            .any(|c| !matches!(c.program.as_str(), "ssh" | "echo" | "printf"))
}
