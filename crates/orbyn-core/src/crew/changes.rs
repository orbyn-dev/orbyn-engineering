//! Which Crew routes change state, so Autopilot's always-denied list can refuse them by path whatever the mode, as it does the
//! account's (`account::changes`). An agent reaches the service as a network action or a command, not through a handler, so the
//! marker is this list, and `orbynd`'s route-table test fails when a route under `/api/crew` is in neither list.
//!
//! The check is by path and ignores the method: any path under `/api/crew/` that is not a listed read counts as a change, so a
//! route nobody listed is refused until someone says it is a read. Spelling tricks (case, `//`, `.`/`..`, a query or trailing
//! slash, percent-encoded letters) do not hide a path.

use crate::account::changes::{normalize, paths_with_prefix, pattern_matches};

/// Every route under `/api/crew` that changes state, as `(method, path)`. A `{id}` segment matches any one segment.
pub const STATE_CHANGING: &[(&str, &str)] = &[
    ("PUT", "/api/crew/autopilot/mode"),
    ("POST", "/api/crew/autopilot/confirm"),
    ("POST", "/api/crew/autopilot/review/{id}/looks-good"),
    ("POST", "/api/crew/autopilot/review/{id}/comment"),
    ("POST", "/api/crew/autopilot/review/{id}/revert"),
    ("POST", "/api/crew/approvals/{id}/approve-once"),
    ("POST", "/api/crew/approvals/{id}/deny"),
    ("POST", "/api/crew/approvals/{id}/dismiss"),
    ("POST", "/api/crew/background/create"),
    ("POST", "/api/crew/background/{id}/update"),
    ("POST", "/api/crew/background/{id}/enable"),
    ("POST", "/api/crew/background/{id}/disable"),
    ("POST", "/api/crew/background/{id}/delete"),
    ("POST", "/api/crew/guard/targets/add"),
    ("POST", "/api/crew/guard/targets/{id}/remove"),
];

/// The reads: they change nothing.
pub const READS: &[(&str, &str)] = &[
    ("GET", "/api/crew/preview"),
    ("GET", "/api/crew/autopilot"),
    ("GET", "/api/crew/autopilot/guards"),
    ("GET", "/api/crew/autopilot/risk/{id}"),
    ("GET", "/api/crew/autopilot/review"),
    ("GET", "/api/crew/autopilot/review/{id}/revert-check"),
    ("GET", "/api/crew/autopilot/merge-queue"),
    ("GET", "/api/crew/autopilot/watch"),
    ("GET", "/api/crew/autopilot/digest"),
    ("GET", "/api/crew/approvals"),
    ("GET", "/api/crew/approvals/held"),
    ("GET", "/api/crew/stops"),
    ("GET", "/api/crew/trust"),
    ("GET", "/api/crew/background"),
    ("GET", "/api/crew/background/{id}"),
    ("GET", "/api/crew/background/{id}/runs"),
    ("GET", "/api/crew/guard/targets"),
    ("GET", "/api/crew/guard/test"),
    ("GET", "/api/crew/classes"),
    ("GET", "/api/crew/classes/test"),
    ("GET", "/api/crew/route/{id}"),
];

const PREFIX: &str = "/api/crew";

/// True when a request to `path` could change Crew state: it is under `/api/crew` and is not a listed read.
pub fn changes_crew_state(path: &str) -> bool {
    let path = normalize(path);
    if !(path == PREFIX || path.starts_with("/api/crew/")) {
        return false;
    }
    STATE_CHANGING
        .iter()
        .any(|(_, route)| pattern_matches(route, &path))
        || !READS.iter().any(|(_, route)| pattern_matches(route, &path))
}

/// Whether text names a Crew route that changes state.
pub fn crew_change(text: &str) -> bool {
    paths_with_prefix(text, PREFIX)
        .iter()
        .any(|path| changes_crew_state(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_change_is_refused_and_every_read_is_not() {
        for (_, route) in STATE_CHANGING {
            assert!(changes_crew_state(&route.replace("{id}", "t-1")), "{route}");
        }
        for (_, route) in READS {
            assert!(
                !changes_crew_state(&route.replace("{id}", "t-1")),
                "{route}"
            );
        }
    }

    #[test]
    fn an_unknown_route_under_crew_counts_as_a_change() {
        assert!(changes_crew_state("/api/crew/new-thing"));
        assert!(changes_crew_state("/api/crew/autopilot/review/t/revert/x"));
        assert!(!changes_crew_state("/api/crews"));
        assert!(!changes_crew_state("/api/projects"));
    }

    #[test]
    fn spelling_tricks_do_not_hide_a_route() {
        for path in [
            "/api/CREW/autopilot/mode",
            "/api//crew/autopilot/mode",
            "/api/./crew/autopilot/mode/",
            "/api/x/../crew/autopilot/mode",
            "/api/%63rew/autopilot/mode",
            "/api/crew/autopilot/mode?x=1",
            "/api/crew/autopilot/review/T-1/REVERT/",
            "/api/crew/autopilot/mo%64e",
        ] {
            assert!(changes_crew_state(path), "{path}");
        }
    }

    #[test]
    fn a_route_is_found_in_a_command_whatever_the_host() {
        assert!(crew_change(
            "curl -X PUT http://127.0.0.1:9/api/crew/autopilot/mode -d '{}'"
        ));
        assert!(!crew_change("curl http://127.0.0.1:9/api/crew/approvals"));
        assert!(!crew_change("ls src"));
    }
}
