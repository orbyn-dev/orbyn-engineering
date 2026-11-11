//! The rules of the always-denied list that read what a program is asked to do: history rewrites
//! (N5), settings and permissions (N6), deploys (N8), publishing and messaging (N9) and payments
//! (N10). Each is a table of programs and their sub-commands, so a reader can see what is refused.

use std::sync::OnceLock;

use orbyn_contracts::ActionKind;
use regex::Regex;

use super::hard::{Ctx, written};
use super::shell::{Simple, hosts};

/// A branch name that is, or may be, the main line.
fn is_main(name: &str) -> bool {
    let name = name.trim_start_matches('+');
    let name = name.rsplit_once(':').map_or(name, |(_, dst)| dst);
    let name = name
        .strip_prefix("refs/heads/")
        .or_else(|| name.strip_prefix("refs/remotes/origin/"))
        .or_else(|| name.strip_prefix("origin/"))
        .unwrap_or(name);
    matches!(name, "main" | "master" | "trunk" | "HEAD" | "")
}

/// Whether a push of `refs` lands on the main line: a ref spec whose destination is main, a bare `HEAD` or no ref while on it.
/// Pushing main is a person's: the merge queue merges locally and nothing here pushes.
fn pushes_to_main(refs: &[&&str], on_main: bool) -> bool {
    if refs.is_empty() {
        return on_main;
    }
    refs.iter().any(|r| {
        let r = r.trim_start_matches('+');
        match r.split_once(':') {
            Some((_, dst)) => is_main(dst) && dst != "HEAD",
            None if r == "HEAD" => on_main,
            None => is_main(r),
        }
    })
}

/// The sub-command of a git command and the words after it. Global options (`-c x`, `-C dir`) are skipped.
fn git_sub(command: &Simple) -> Option<(&str, Vec<&str>)> {
    let mut args = command.args.iter().map(String::as_str);
    while let Some(arg) = args.next() {
        match arg {
            "-c" | "-C" | "--git-dir" | "--work-tree" | "--namespace" => {
                args.next();
            }
            flag if flag.starts_with('-') => {}
            sub => return Some((sub, args.collect())),
        }
    }
    None
}

fn is_git(command: &Simple) -> bool {
    command.program == "git"
}

/// N5: a force-push or delete aimed at main, or a history rewrite while on it.
pub(super) fn n5(ctx: &Ctx<'_>) -> bool {
    let on_main = ctx.scene.branch.is_none_or(is_main);
    ctx.commands.iter().filter(|c| is_git(c)).any(|command| {
        let Some((sub, rest)) = git_sub(command) else {
            return false;
        };
        let flags = |names: &[&str]| {
            rest.iter().any(|w| {
                names.contains(w) || names.iter().any(|n| n.ends_with('=') && w.starts_with(n))
            })
        };
        let named: Vec<&str> = rest
            .iter()
            .copied()
            .filter(|w| !w.starts_with('-'))
            .collect();
        match sub {
            "push" => {
                let force = flags(&[
                    "--force",
                    "-f",
                    "--force-with-lease=",
                    "--force-with-lease",
                    "--force-if-includes",
                    "--mirror",
                    "--prune",
                ]) || named.iter().any(|w| w.starts_with('+'));
                let delete = flags(&["--delete", "-d"]) || named.iter().any(|w| w.starts_with(':'));
                let refs: Vec<&&str> = named.iter().skip(1).collect();
                ((force || delete) && (refs.is_empty() || refs.iter().any(|r| is_main(r))))
                    || pushes_to_main(&refs, on_main)
            }
            "filter-branch" | "filter-repo" => true,
            "reflog" => rest.iter().any(|w| matches!(*w, "expire" | "delete")),
            "update-ref" => {
                rest.contains(&"-d") || rest.iter().any(|w| is_main(w) && w.contains("refs/"))
            }
            "rebase" | "replace" => on_main,
            "commit" => on_main && flags(&["--amend"]),
            "reset" => {
                on_main
                    && (flags(&["--hard", "--soft", "--mixed", "--keep", "--merge"])
                        || named.iter().any(|w| {
                            w.starts_with("HEAD~")
                                || w.starts_with("HEAD^")
                                || w.starts_with("origin/")
                        }))
            }
            "branch" => {
                flags(&["-f", "--force", "-D", "-d", "-m", "-M", "--delete"])
                    && named.iter().any(|w| is_main(w))
            }
            "checkout" => flags(&["-B"]) && named.iter().any(|w| is_main(w)),
            "switch" => flags(&["-C", "--force-create"]) && named.iter().any(|w| is_main(w)),
            _ => false,
        }
    })
}

const PROTECTION_WORDS: [&str; 14] = [
    "protection",
    "ruleset",
    "collaborators",
    "permissions",
    "settings",
    "hooks",
    "keys",
    "secrets",
    "variables",
    "environments",
    "invitations",
    "teams",
    "rules",
    "actions",
];

/// N6: branch protection, permissions and settings, of the forge, git, the machine or Orbyn.
pub(super) fn n6(ctx: &Ctx<'_>) -> bool {
    let lower = ctx.text.to_lowercase();
    if lower.contains("/api/settings")
        || lower.contains("/api/policy")
        || lower.contains("/api/permission")
    {
        return true;
    }
    if written(ctx)
        .iter()
        .any(|p| p == ".git/config" || p.starts_with(".git/hooks") || p == ".git/info/exclude")
    {
        return true;
    }
    ctx.commands.iter().any(|command| {
        let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
        let words: Vec<&str> = args
            .iter()
            .copied()
            .filter(|w| !w.starts_with('-'))
            .collect();
        match command.program.as_str() {
            "sudo" | "doas" | "su" | "icacls" | "setfacl" | "chcon" | "visudo" | "usermod"
            | "passwd" | "useradd" | "chpasswd" | "setx" | "launchctl" | "systemctl"
            | "crontab" => true,
            "defaults" | "reg" => words
                .iter()
                .any(|w| matches!(*w, "write" | "delete" | "add" | "import")),
            "npm" | "pnpm" | "yarn" => {
                words.first() == Some(&"config")
                    && words
                        .iter()
                        .any(|w| matches!(*w, "set" | "delete" | "edit"))
            }
            "git" => {
                let Some((sub, rest)) = git_sub(command) else {
                    return false;
                };
                let plain: Vec<&&str> = rest.iter().filter(|w| !w.starts_with('-')).collect();
                sub == "config"
                    && (plain.len() >= 2
                        || rest.iter().any(|w| {
                            matches!(
                                *w,
                                "--add"
                                    | "--unset"
                                    | "--unset-all"
                                    | "--replace-all"
                                    | "--edit"
                                    | "-e"
                                    | "--remove-section"
                                    | "--rename-section"
                            )
                        }))
            }
            "gh" => {
                let sub = words.first().copied();
                let verb = words.get(1).copied();
                match sub {
                    Some("repo") => matches!(
                        verb,
                        Some("edit" | "delete" | "archive" | "rename" | "transfer")
                    ),
                    Some("secret" | "variable") => {
                        matches!(verb, Some("set" | "delete" | "remove"))
                    }
                    Some("ruleset") => true,
                    Some("auth") => matches!(
                        verb,
                        Some("login" | "logout" | "refresh" | "setup-git" | "switch")
                    ),
                    Some("api") => {
                        let writing = args.iter().any(|w| {
                            matches!(*w, "-f" | "-F" | "--field" | "--raw-field" | "--input")
                        }) || args.windows(2).any(|w| {
                            matches!(w[0], "-X" | "--method") && !w[1].eq_ignore_ascii_case("get")
                        }) || args.iter().any(|w| {
                            w.starts_with("--method=") && !w.eq_ignore_ascii_case("--method=get")
                        });
                        let all = args.join(" ").to_lowercase();
                        writing && PROTECTION_WORDS.iter().any(|w| all.contains(w))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    })
}

/// A production target: a deploy to it is never an agent's to approve.
pub(super) fn n8(ctx: &Ctx<'_>) -> bool {
    ctx.commands.iter().any(|command| {
        let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
        let has = |w: &str| args.contains(&w);
        let any = |ws: &[&str]| ws.iter().any(|w| has(w));
        match command.program.as_str() {
            "kubectl" | "oc" => any(&[
                "apply", "delete", "rollout", "scale", "patch", "replace", "create", "set",
                "drain", "cordon", "annotate", "label",
            ]),
            "terraform" | "tofu" | "pulumi" | "cdk" => any(&["apply", "destroy", "deploy", "up"]),
            "helm" | "helmfile" => any(&[
                "install",
                "upgrade",
                "rollback",
                "uninstall",
                "sync",
                "apply",
            ]),
            "vercel" => has("--prod") || has("promote"),
            "netlify" => has("deploy") && (has("--prod") || has("-p")),
            "fly" | "flyctl" | "serverless" | "sls" | "firebase" | "wrangler" | "railway"
            | "render" | "eb" | "cf" => any(&["deploy", "publish", "up", "push", "release"]),
            "heroku" => args
                .iter()
                .any(|a| a.starts_with("releases:") || a.starts_with("container:release")),
            "aws" => {
                any(&[
                    "deploy",
                    "update-service",
                    "update-function-code",
                    "create-deployment",
                    "put-rule",
                    "sync",
                ]) && !has("--dryrun")
            }
            "gcloud" | "az" => any(&["deploy", "release", "rollout"]),
            "gh" => {
                has("workflow")
                    && has("run")
                    && args.iter().any(|a| {
                        let a = a.to_lowercase();
                        a.contains("deploy") || a.contains("release") || a.contains("prod")
                    })
            }
            "git" => git_sub(command).is_some_and(|(sub, rest)| {
                sub == "push"
                    && rest
                        .iter()
                        .filter(|w| !w.starts_with('-'))
                        .skip(1)
                        .any(|r| {
                            let r = r
                                .trim_start_matches('+')
                                .rsplit(':')
                                .next()
                                .unwrap_or(r)
                                .to_lowercase();
                            r.contains("prod")
                                || r.starts_with("release")
                                || r.starts_with("deploy")
                        })
            }),
            _ => false,
        }
    })
}

fn messaging_hosts() -> &'static [&'static str] {
    &[
        "slack.com",
        "hooks.slack.com",
        "discord.com",
        "discordapp.com",
        "api.telegram.org",
        "api.twilio.com",
        "api.sendgrid.com",
        "api.mailgun.net",
        "graph.facebook.com",
        "api.twitter.com",
        "api.x.com",
        "outlook.office.com",
        "api.linkedin.com",
        "api.postmarkapp.com",
        "api.resend.com",
    ]
}

fn tool_names() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(slack|discord|telegram|twilio|sendgrid|mailgun|gmail|email|smtp|sms|tweet|linkedin).*(send|post|publish|message|reply|create|write)|(send|post|publish)[_-]?(message|email|mail|tweet|sms)")
            .expect("a fixed pattern")
    })
}

/// N9: a package or image published, a release cut, a message sent to someone outside the repository.
pub(super) fn n9(ctx: &Ctx<'_>) -> bool {
    let named_host = |host: &str| {
        messaging_hosts()
            .iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")))
    };
    if ctx.kind != ActionKind::Command
        && ctx.kind != ActionKind::FileChange
        && tool_names().is_match(&ctx.text)
    {
        return true;
    }
    ctx.commands.iter().any(|command| {
        let args: Vec<&str> = command.args.iter().map(String::as_str).collect();
        let has = |w: &str| args.contains(&w);
        let publishing = match command.program.as_str() {
            "npm" | "pnpm" | "yarn" | "bun" | "cargo" | "gem" | "dotnet" => {
                has("publish") || has("push") && command.program != "cargo"
            }
            "twine" => has("upload"),
            "mvn" | "gradle" | "gradlew" => has("deploy") || has("publish"),
            "docker" | "podman" => has("push"),
            "gh" => {
                has("release")
                    && args
                        .iter()
                        .any(|a| matches!(*a, "create" | "upload" | "edit" | "delete"))
                    || has("gist") && has("create")
            }
            "mail" | "mailx" | "sendmail" | "mutt" | "msmtp" | "swaks" => true,
            _ => false,
        };
        publishing || hosts(command).iter().any(|h| named_host(h))
    })
}

fn payment_hosts() -> &'static [&'static str] {
    &[
        "stripe.com",
        "paypal.com",
        "braintreegateway.com",
        "adyen.com",
        "paddle.com",
        "lemonsqueezy.com",
        "razorpay.com",
        "klarna.com",
        "checkout.com",
        "squareup.com",
        "venmo.com",
        "paymentsos.com",
    ]
}

fn payment_words() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)(^|[^a-z])(stripe|paypal|braintree|adyen|payment|checkout|purchase|charge|refund|payout|buy[_ -]?now|place[_ -]?order|pay[_ -]?now)([^a-z]|$)")
            .expect("a fixed pattern")
    })
}

/// N10: a payment host, a payment program, or a tool or page that takes money.
pub(super) fn n10(ctx: &Ctx<'_>) -> bool {
    let named_host = |host: &str| {
        payment_hosts()
            .iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")))
    };
    let page_or_tool = matches!(
        ctx.kind,
        ActionKind::Tool | ActionKind::Browser | ActionKind::Computer | ActionKind::Network
    ) && payment_words().is_match(&ctx.text);
    page_or_tool
        || ctx.commands.iter().any(|command| {
            matches!(command.program.as_str(), "stripe" | "paypal" | "braintree")
                || hosts(command).iter().any(|h| named_host(h))
        })
        || ctx.text.contains("://") && {
            let simple = Simple {
                program: String::new(),
                raw: String::new(),
                args: ctx.text.split_whitespace().map(str::to_owned).collect(),
                redirects: Vec::new(),
            };
            hosts(&simple).iter().any(|h| named_host(h))
        }
}
