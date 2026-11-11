//! Which account routes change account, device or sign-in state, so Autopilot's always-denied
//! list (SPEC.md N1, decision D12) can refuse them by policy whatever the mode. Governance sees a
//! command or a request, not the route's handler, so the marker is this list, and
//! `orbynd`'s route-table test fails when a route under `/api/account` is in neither list.
//!
//! The check is by path and ignores the method: an agent that sends the wrong method to a
//! state-changing path is still refused, and a path under `/api/account/` that this module does
//! not know is treated as changing state until someone lists it as a read.

/// Every route under `/api/account` that changes state, as `(method, path)`. A `{id}` segment matches any one segment.
pub const STATE_CHANGING: &[(&str, &str)] = &[
    ("DELETE", "/api/account/devices/{id}"),
    ("POST", "/api/account/approvals/{id}/approve"),
    ("POST", "/api/account/approvals/{id}/deny"),
    ("POST", "/api/account/confirm"),
    ("POST", "/api/account/confirm/cancel"),
    ("POST", "/api/account/connect"),
    ("POST", "/api/account/connect/later"),
    ("POST", "/api/account/connect/seen"),
    ("POST", "/api/account/connect/use-account"),
    ("POST", "/api/account/connect/use-device"),
    ("POST", "/api/account/ended/seen"),
    ("POST", "/api/account/sign-in"),
    ("POST", "/api/account/sign-in/approval"),
    ("POST", "/api/account/sign-in/cancel"),
    ("POST", "/api/account/sign-in/second-factor"),
    ("POST", "/api/account/sign-out"),
    ("POST", "/api/account/sign-out-everywhere"),
    ("POST", "/api/account/sign-up/code"),
    ("POST", "/api/account/sign-up/invite"),
    ("POST", "/api/account/sign-up/register"),
    ("POST", "/api/account/sign-up/resend"),
    ("POST", "/api/account/sign-up/start"),
    ("POST", "/api/account/sync/now"),
    ("POST", "/api/account/sync/pause"),
    ("POST", "/api/account/sync/resume"),
];

/// The reads: they change nothing.
pub const READS: &[(&str, &str)] = &[
    ("GET", "/api/account"),
    ("GET", "/api/account/devices"),
    ("GET", "/api/account/approvals"),
];

const PREFIX: &str = "/api/account";

fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, path) = (segments(pattern), segments(path));
    pattern.len() == path.len()
        && pattern
            .iter()
            .zip(&path)
            .all(|(want, got)| *want == "{id}" || want == got)
}

/// `text` with the percent-encoding of letters, digits and `-._~` undone.
fn decode_unreserved(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let pair = (bytes[at] == b'%' && at + 2 < bytes.len())
            .then(|| std::str::from_utf8(&bytes[at + 1..at + 3]).ok())
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            .filter(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
            });
        match pair {
            Some(byte) => {
                decoded.push(byte);
                at += 3;
            }
            None => {
                decoded.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// The path as the account routes would read it: lowercase, no query or fragment, no `.` or `..`
/// segment, no repeated slash, and the percent-encoding of letters, digits and `-._~` undone, so
/// `%61ccount` is `account`. A path that does not start at `/api/account` after that is not one.
pub fn normalize(path: &str) -> String {
    let end = path.find(['?', '#']).unwrap_or(path.len());
    let lower = decode_unreserved(&path[..end]).to_lowercase();
    let mut kept: Vec<&str> = Vec::new();
    for part in lower.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                kept.pop();
            }
            other => kept.push(other),
        }
    }
    format!("/{}", kept.join("/"))
}

/// Whether `path` is under the account API at all.
pub fn is_account_path(path: &str) -> bool {
    let path = normalize(path);
    path == PREFIX || path.starts_with("/api/account/")
}

/// True when a request to `path` could change account, device or sign-in state: the path is a
/// listed state-changing route, or is under the account API and is not a listed read.
pub fn changes_account_state(path: &str) -> bool {
    let path = normalize(path);
    if !(path == PREFIX || path.starts_with("/api/account/")) {
        return false;
    }
    STATE_CHANGING
        .iter()
        .any(|(_, route)| matches(route, &path))
        || !READS.iter().any(|(_, route)| matches(route, &path))
}

/// The account-API paths a piece of text names: each occurrence of `/api/account` up to the end of
/// its path. A host in front, quotes and a scheme do not matter.
pub fn account_paths_in(text: &str) -> Vec<String> {
    let text = &decode_unreserved(text);
    let lower = text.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find(PREFIX) {
        let start = from + at;
        let rest = &text[start..];
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ')' | ',' | ';'))
            .unwrap_or(rest.len());
        found.push(rest[..end].to_owned());
        from = start + PREFIX.len();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_state_change_is_refused_and_every_read_is_not() {
        for (_, route) in STATE_CHANGING {
            let concrete = route.replace("{id}", "dev-1");
            assert!(changes_account_state(&concrete), "a listed route");
        }
        for (_, route) in READS {
            assert!(!changes_account_state(route), "a read");
        }
    }

    #[test]
    fn an_unknown_route_under_the_account_api_counts_as_a_change() {
        assert!(changes_account_state("/api/account/new-thing"));
        assert!(changes_account_state("/api/account/devices/x/extra"));
        assert!(!changes_account_state("/api/projects"));
        assert!(!changes_account_state("/api/accounting"));
    }

    #[test]
    fn spelling_tricks_do_not_hide_a_route() {
        for path in [
            "/api/Account/sign-out",
            "/api//account/sign-out",
            "/api/./account/sign-out/",
            "/api/x/../account/sign-out",
            "/api/%61ccount/sign-out",
            "/api/account/sign-out?x=1",
            "/api/account/sign%2Dout",
        ] {
            assert!(changes_account_state(path), "{path}");
        }
    }

    #[test]
    fn paths_are_found_in_a_command_whatever_the_host() {
        let found =
            account_paths_in("curl -X POST http://127.0.0.1:9/api/account/sign-out -d '{}'");
        assert_eq!(found, ["/api/account/sign-out"]);
        assert!(account_paths_in("ls src").is_empty());
    }
}
