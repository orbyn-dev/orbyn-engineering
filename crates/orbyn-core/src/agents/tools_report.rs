//! Helpers for the lists of tools and connectors an engine reports. Only names, states and
//! short reasons are kept; an address, token or request body never leaves the engine's reply.

use orbyn_contracts::{EngineTools, ToolEntry, ToolGroup, ToolState};

const REASON_CHARS: usize = 140;
const DETAIL_CHARS: usize = 1500;
const DESCRIPTION_CHARS: usize = 120;

/// Words that name a credential: the word after one of these, written `name: value`, is hidden.
const SECRET_NAMES: &[&str] = &[
    "token",
    "key",
    "secret",
    "password",
    "authorization",
    "bearer",
    "cookie",
];

/// One address, found anywhere in `word` (after a bracket or quote), cut to scheme and host:
/// any user and password in front of the host go, and so do the path, query and fragment.
fn trim_address(word: &str) -> String {
    let lower = word.to_ascii_lowercase();
    let Some(start) = ["https://", "http://"]
        .iter()
        .filter_map(|scheme| lower.find(scheme))
        .min()
    else {
        return word.to_owned();
    };
    let scheme_end = start + word[start..].find("://").map_or(0, |at| at + 3);
    let rest = &word[scheme_end..];
    // The address runs to the first character an address cannot hold; what follows
    // (a closing bracket, a quote, a full stop) belongs to the sentence and stays.
    let url_end = rest
        .find(|c: char| !(c.is_alphanumeric() || "/?#=&%._~:@+-![]*'$,;".contains(c)))
        .unwrap_or(rest.len());
    let (url, after) = rest.split_at(url_end);
    let authority_end = url.find(['/', '?', '#']).unwrap_or(url.len());
    let authority = &url[..authority_end];
    // `user:password@host` keeps only `host`.
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{}{}{}", &word[..scheme_end], host, after)
}

/// `text` with every address cut down to its scheme and host, and anything that looks
/// like a credential masked, so a server's own error message can be shown without the
/// user, path, query or token it came with.
pub(crate) fn redact(text: &str) -> String {
    let mut out = Vec::new();
    let mut hide_next = false;
    for word in text.split_inclusive(char::is_whitespace) {
        let bare = word.trim_end();
        let tail = &word[bare.len()..];
        let lower = bare.to_ascii_lowercase();
        let cleaned = if hide_next && !bare.is_empty() {
            "[hidden]".to_owned()
        } else if lower.contains("http://") || lower.contains("https://") {
            trim_address(bare)
        } else if [
            "token=",
            "key=",
            "secret=",
            "password=",
            "authorization=",
            "cookie=",
        ]
        .iter()
        .any(|name| lower.contains(name))
        {
            let at = bare.find('=').map_or(bare.len(), |at| at + 1);
            format!("{}[hidden]", &bare[..at])
        } else {
            bare.to_owned()
        };
        // `Authorization: Bearer abc`, `token: abc`, `X-Api-Key: abc`: a credential name
        // ending in a colon (or `bearer` on its own) hides the word after it.
        let name = lower.trim_end_matches(':');
        let names_secret = SECRET_NAMES.iter().any(|secret| name.ends_with(secret));
        hide_next =
            !bare.is_empty() && (lower == "bearer" || (lower.ends_with(':') && names_secret));
        out.push(format!("{cleaned}{tail}"));
    }
    // Whatever the word rules missed, a recognised secret format never gets through.
    crate::redaction::Redactor::locked().replace_each(&out.concat(), "[hidden]")
}

/// The first sentence or line of `text`, cut to `max` characters.
pub(crate) fn short(text: &str, max: usize) -> String {
    let text = redact(text);
    let line = text.lines().next().unwrap_or("").trim();
    let line = line.split_once(". ").map_or(line, |(first, _)| first);
    let mut out: String = line.chars().take(max).collect();
    if line.chars().count() > max {
        out.push('…');
    }
    out
}

pub(crate) fn reason(text: &str) -> String {
    short(text, REASON_CHARS)
}

pub(crate) fn description(text: &str) -> Option<String> {
    let line = short(text, DESCRIPTION_CHARS);
    (!line.is_empty()).then_some(line)
}

pub(crate) fn detail(text: &str) -> String {
    redact(text).chars().take(DETAIL_CHARS).collect()
}

pub(crate) fn entry(name: &str, state: ToolState) -> ToolEntry {
    ToolEntry {
        name: name.to_owned(),
        description: None,
        state,
        reason: None,
        detail: None,
        source: None,
        tools: None,
        sign_in_url: None,
    }
}

pub(crate) fn group(id: &str, items: Vec<ToolEntry>) -> ToolGroup {
    ToolGroup {
        id: id.to_owned(),
        count: u32::try_from(items.len()).unwrap_or(u32::MAX),
        items,
    }
}

/// A group that is listed by count alone, such as Orbyn's own tools.
pub(crate) fn counted(id: &str, count: usize) -> ToolGroup {
    ToolGroup {
        id: id.to_owned(),
        count: u32::try_from(count).unwrap_or(u32::MAX),
        items: Vec::new(),
    }
}

pub(crate) const ALL_GROUPS: &[&str] =
    &["builtin", "orbyn", "mcp", "connectors", "plugins", "skills"];

/// Orders the groups the engine reported and names the ones it did not.
pub(crate) fn assemble(
    engine: &str,
    source: &str,
    mut groups: Vec<ToolGroup>,
    reported: bool,
) -> EngineTools {
    groups.sort_by_key(|group| {
        ALL_GROUPS
            .iter()
            .position(|id| *id == group.id)
            .unwrap_or(usize::MAX)
    });
    let not_reported = ALL_GROUPS
        .iter()
        .filter(|id| !groups.iter().any(|group| group.id == **id))
        .map(|id| (*id).to_owned())
        .collect();
    EngineTools {
        engine: engine.to_owned(),
        source: source.to_owned(),
        checked_at: Some(crate::util::now()),
        groups,
        not_reported,
        error: None,
        reported,
        incomplete: false,
    }
}

/// Everything an engine's tools check keeps between calls: one read for the engine's server
/// list and one for Codex's reduced lists. One of these lives per engine id in `Core`, so an
/// engine instance, which the registry rebuilds as a fresh driver on every lookup, shares its
/// read, its 30 seconds and its short memory of a failure with every thread that asks.
#[derive(Default)]
pub struct ToolsProbe {
    pub(crate) claude: Probed<ListedServers>,
    pub(crate) codex: Probed<orbyn_contracts::EngineTools>,
}

/// `claude mcp list`, parsed, with the time it was read.
#[derive(Clone)]
pub struct ListedServers {
    pub(crate) at: String,
    pub(crate) servers: Vec<crate::agents::driver::claude::McpListed>,
}

/// An engine-wide answer kept for a short while. Callers that arrive while it is being
/// read wait for that read instead of starting their own, and a read that failed is not
/// repeated at once, so opening a dozen threads asks the engine once.
pub(crate) struct Probed<T> {
    state: tokio::sync::Mutex<ProbedState<T>>,
}

struct ProbedState<T> {
    answer: Option<(std::time::Instant, T)>,
    failed: Option<std::time::Instant>,
}

impl<T> Default for Probed<T> {
    fn default() -> Self {
        Self {
            state: tokio::sync::Mutex::new(ProbedState {
                answer: None,
                failed: None,
            }),
        }
    }
}

/// How long a failed read is remembered, so queued callers do not each wait out a timeout.
const FAILED_FOR: std::time::Duration = std::time::Duration::from_secs(5);

impl<T: Clone> Probed<T> {
    /// The kept answer while it is younger than `keep`, else `read`'s. `refresh` skips what is kept.
    pub(crate) async fn get<F, Fut>(
        &self,
        refresh: bool,
        keep: std::time::Duration,
        read: F,
    ) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Option<T>>,
    {
        let mut state = self.state.lock().await;
        if !refresh {
            if let Some((at, answer)) = &state.answer
                && at.elapsed() < keep
            {
                return Some(answer.clone());
            }
            if state.failed.is_some_and(|at| at.elapsed() < FAILED_FOR) {
                return None;
            }
        }
        match read().await {
            Some(answer) => {
                state.answer = Some((std::time::Instant::now(), answer.clone()));
                state.failed = None;
                Some(answer)
            }
            None => {
                state.failed = Some(std::time::Instant::now());
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cut_to_a_short_plain_line() {
        assert_eq!(short("First line. Second.\nThird", 100), "First line");
        assert_eq!(short(&"x".repeat(200), 10), format!("{}…", "x".repeat(10)));
        assert_eq!(description("   "), None);
    }

    #[test]
    fn groups_come_out_in_display_order_and_the_missing_are_named() {
        let tools = assemble(
            "codex",
            "check",
            vec![counted("plugins", 1), counted("mcp", 2)],
            true,
        );
        let ids: Vec<_> = tools.groups.iter().map(|group| group.id.as_str()).collect();
        assert_eq!(ids, ["mcp", "plugins"]);
        assert_eq!(
            tools.not_reported,
            ["builtin", "orbyn", "connectors", "skills"]
        );
    }

    #[test]
    fn addresses_lose_their_path_and_query_and_credentials_are_masked() {
        let text = redact(
            "HTTP 502 from https://mcp.example.com/v1/x?token=abc123&u=1 (see http://host:8080/a#b) Bearer sk-secret then password=hunter2",
        );
        assert!(text.contains("https://mcp.example.com"), "{text}");
        assert!(text.contains("http://host:8080"), "{text}");
        for leak in ["abc123", "/v1/x", "sk-secret", "hunter2", "#b"] {
            assert!(!text.contains(leak), "{leak} in {text}");
        }
        assert_eq!(
            redact("Command not found: pg-mcp"),
            "Command not found: pg-mcp"
        );
    }

    #[test]
    fn user_and_password_in_front_of_a_host_are_dropped() {
        let text =
            redact("failed: https://user:pass@host.example/x and http://me@other.example:9/y");
        assert!(text.contains("https://host.example"), "{text}");
        assert!(text.contains("http://other.example:9"), "{text}");
        for leak in ["user", "pass", "me@", "/x", "/y"] {
            assert!(!text.contains(leak), "{leak} in {text}");
        }
    }

    #[test]
    fn an_address_after_a_bracket_or_quote_is_cut_too() {
        for open in ["(", "\"", "<", "[", "'"] {
            let text = redact(&format!(
                "see {open}https://host.example/secret/path?token=zzz and more"
            ));
            assert!(text.contains("https://host.example"), "{open}: {text}");
            assert!(
                !text.contains("secret/path") && !text.contains("zzz"),
                "{open}: {text}"
            );
        }
        assert_eq!(redact("<https://h.example/p>"), "<https://h.example>");
    }

    #[test]
    fn a_credential_written_as_name_colon_value_is_hidden() {
        for line in [
            "token: abc123 next",
            "X-Api-Key: abc123 next",
            "Authorization: Bearer abc123 next",
            "password: abc123 next",
            "secret: abc123 next",
        ] {
            let text = redact(line);
            assert!(!text.contains("abc123"), "{line} -> {text}");
            assert!(text.ends_with("next"), "{text}");
        }
        assert_eq!(
            redact("Command not found: pg-mcp"),
            "Command not found: pg-mcp"
        );
        assert_eq!(redact("key not found in cache"), "key not found in cache");
    }

    #[tokio::test]
    async fn a_probed_answer_is_read_once_for_many_callers_until_refreshed() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let reads = std::sync::Arc::new(AtomicUsize::new(0));
        let probed = std::sync::Arc::new(Probed::<u32>::default());
        let keep = std::time::Duration::from_secs(30);
        let one = |probed: std::sync::Arc<Probed<u32>>,
                   reads: std::sync::Arc<AtomicUsize>,
                   refresh| async move {
            probed
                .get(refresh, keep, || async move {
                    reads.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    Some(7)
                })
                .await
        };
        let all = futures_util::future::join_all(
            (0..6).map(|_| one(probed.clone(), reads.clone(), false)),
        )
        .await;
        assert!(all.iter().all(|value| *value == Some(7)));
        assert_eq!(reads.load(Ordering::SeqCst), 1, "six callers, one read");
        one(probed.clone(), reads.clone(), false).await;
        assert_eq!(reads.load(Ordering::SeqCst), 1, "kept");
        one(probed.clone(), reads.clone(), true).await;
        assert_eq!(reads.load(Ordering::SeqCst), 2, "a refresh reads again");
    }

    #[tokio::test]
    async fn a_failed_read_is_not_repeated_by_the_callers_queued_behind_it() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let reads = std::sync::Arc::new(AtomicUsize::new(0));
        let probed = std::sync::Arc::new(Probed::<u32>::default());
        let keep = std::time::Duration::from_secs(30);
        let attempt = |refresh: bool| {
            let (probed, reads) = (probed.clone(), reads.clone());
            async move {
                probed
                    .get(refresh, keep, || async move {
                        reads.fetch_add(1, Ordering::SeqCst);
                        None
                    })
                    .await
            }
        };
        let all = futures_util::future::join_all((0..4).map(|_| attempt(false))).await;
        assert!(all.iter().all(Option::is_none));
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        attempt(true).await;
        assert_eq!(
            reads.load(Ordering::SeqCst),
            2,
            "asking again does try again"
        );
    }
}
