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
    ("POST", "/api/crew/tasks/{id}/merge"),
    ("POST", "/api/crew/backtest/start"),
    ("POST", "/api/crew/backtest/{id}/stop"),
    ("POST", "/api/crew/settings/engine-tiers/save"),
    ("POST", "/api/crew/capacity/settings"),
    ("POST", "/api/crew/capacity/tasks/{id}"),
    ("POST", "/api/crew/review/{id}/consent"),
    ("POST", "/api/crew/review/{id}/remove-lane"),
    ("PUT", "/api/crew/autopilot/mode"),
    ("POST", "/api/crew/autopilot/digest/schedule/set"),
    ("POST", "/api/crew/autopilot/confirm"),
    ("POST", "/api/crew/autopilot/areas/override"),
    ("POST", "/api/crew/autopilot/areas/follow"),
    ("POST", "/api/crew/autopilot/merge-queue/{id}/resolve"),
    ("POST", "/api/crew/autopilot/review/{id}/looks-good"),
    ("POST", "/api/crew/autopilot/review/{id}/comment"),
    ("POST", "/api/crew/autopilot/review/{id}/revert"),
    ("POST", "/api/crew/approvals/{id}/approve-once"),
    ("POST", "/api/crew/approvals/{id}/deny"),
    ("POST", "/api/crew/approvals/{id}/dismiss"),
    ("POST", "/api/crew/approvals/second-engine/{id}/dismiss"),
    ("POST", "/api/crew/background/create"),
    ("POST", "/api/crew/background/{id}/update"),
    ("POST", "/api/crew/background/{id}/enable"),
    ("POST", "/api/crew/background/{id}/disable"),
    ("POST", "/api/crew/background/{id}/delete"),
    ("POST", "/api/crew/background/{id}/run-now"),
    ("POST", "/api/crew/tasks/{id}/rerun-checks"),
    ("POST", "/api/crew/tasks/{id}/cancel"),
    ("POST", "/api/crew/tasks/{id}/retry-elsewhere"),
    ("POST", "/api/crew/guard/targets/add"),
    ("POST", "/api/crew/guard/targets/{id}/remove"),
    ("POST", "/api/crew/escapes/mark"),
    ("POST", "/api/crew/escapes/{id}/confirm"),
    ("POST", "/api/crew/escapes/{id}/dismiss"),
    ("POST", "/api/crew/escapes/{id}/gap"),
    ("POST", "/api/crew/escapes/{id}/check"),
    ("POST", "/api/crew/escapes/{id}/check/accept"),
    ("POST", "/api/crew/escapes/{id}/check/retry"),
    ("POST", "/api/crew/escapes/{id}/check/withdraw"),
    ("POST", "/api/crew/escapes/{id}/check/approve"),
    ("POST", "/api/crew/projects/{id}/pinned-ref/confirm"),
    ("POST", "/api/crew/projects/{id}/pinned-ref/reset"),
    ("POST", "/api/crew/projects/{id}/worker-provider/set"),
    ("POST", "/api/crew/profile/apply"),
];

/// Reads confer no action authority. The notice projection may record observed
/// second-engine availability, never approve or retry work.
pub const READS: &[(&str, &str)] = &[
    ("GET", "/api/crew/settings/engine-tiers"),
    ("GET", "/api/crew/review/{id}/controls"),
    ("GET", "/api/crew/backtest/estimate"),
    ("GET", "/api/crew/backtest/start-view"),
    ("GET", "/api/crew/backtest/{id}/report"),
    ("GET", "/api/crew/capacity"),
    ("GET", "/api/crew/capacity/coverage"),
    ("GET", "/api/crew/preview"),
    ("GET", "/api/crew/autopilot"),
    ("GET", "/api/crew/autopilot/guards"),
    ("GET", "/api/crew/autopilot/risk/{id}"),
    ("GET", "/api/crew/autopilot/review"),
    ("GET", "/api/crew/autopilot/review/{id}/revert-check"),
    ("GET", "/api/crew/autopilot/merge-queue"),
    ("GET", "/api/crew/autopilot/watch"),
    ("GET", "/api/crew/autopilot/digest"),
    ("GET", "/api/crew/autopilot/digest/schedule"),
    ("GET", "/api/crew/autopilot/areas"),
    ("GET", "/api/crew/autopilot/areas/history"),
    ("GET", "/api/crew/autopilot/thresholds"),
    ("GET", "/api/crew/approvals"),
    ("GET", "/api/crew/approvals/held"),
    ("GET", "/api/crew/approvals/second-engine"),
    ("GET", "/api/crew/stops"),
    ("GET", "/api/crew/trust"),
    ("GET", "/api/crew/background"),
    ("GET", "/api/crew/background/{id}"),
    ("GET", "/api/crew/background/{id}/runs"),
    ("GET", "/api/crew/guard/targets"),
    ("GET", "/api/crew/guard/test"),
    ("GET", "/api/crew/classes"),
    ("GET", "/api/crew/classes/test"),
    ("GET", "/api/crew/projects/{id}/pinned-ref"),
    ("GET", "/api/crew/projects/{id}/worker-provider"),
    ("GET", "/api/crew/route/{id}"),
    ("GET", "/api/crew/escapes"),
    ("GET", "/api/crew/escapes/thresholds"),
    ("GET", "/api/crew/escapes/possible"),
    ("GET", "/api/crew/escapes/{id}"),
    ("GET", "/api/crew/flow/board"),
    ("GET", "/api/crew/flow/crews"),
    ("GET", "/api/crew/flow/facts"),
    ("GET", "/api/crew/flow/map"),
    ("GET", "/api/crew/flow/messages"),
    ("GET", "/api/crew/flow/tasks"),
    ("GET", "/api/crew/flow/tasks/{id}"),
    ("GET", "/api/crew/flow/tasks/{id}/run"),
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
    fn an_agents_request_to_a_capacity_write_is_refused_by_path_and_the_reads_are_not() {
        for path in [
            "/api/crew/capacity/settings",
            "/api/crew/capacity/tasks/t-1",
        ] {
            assert!(changes_crew_state(path), "{path}");
            assert!(
                crew_change(&format!("curl -X POST http://127.0.0.1:9{path} -d x")),
                "{path}"
            );
        }
        for path in ["/api/crew/capacity", "/api/crew/capacity/coverage"] {
            assert!(!changes_crew_state(path), "{path}");
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
