//! Which Settings › Secrets routes change state, so Autopilot's always-denied list can refuse them by path whatever the mode,
//! as it does the account's and Crew's. An agent holding the service token reaches the service as a command or a request, not
//! through a handler, and turning redaction off is the person's answer to give. `orbynd`'s route-table test fails when a route
//! under `/api/settings/secrets` is in neither list.
//!
//! Any path under the prefix that is not a listed read counts as a change.

use crate::account::changes::{normalize, paths_with_prefix, pattern_matches};

/// Every route under `/api/settings/secrets` that changes state, as `(method, path)`. A `{id}` segment matches any one segment.
pub const STATE_CHANGING: &[(&str, &str)] = &[
    ("PUT", "/api/settings/secrets/surfaces"),
    ("PUT", "/api/settings/secrets/formats/{id}"),
    ("PUT", "/api/settings/secrets/projects/{id}"),
    ("POST", "/api/settings/secrets/patterns"),
    ("PUT", "/api/settings/secrets/patterns/{id}"),
    ("DELETE", "/api/settings/secrets/patterns/{id}"),
];

/// The reads: they change nothing. Two share a path with a change, so the path check refuses them too.
pub const READS: &[(&str, &str)] = &[
    ("GET", "/api/settings/secrets"),
    ("GET", "/api/settings/secrets/patterns/{id}"),
    ("POST", "/api/settings/secrets/patterns/test"),
];

const PREFIX: &str = "/api/settings/secrets";

/// True when a request to `path` could change the redaction settings.
pub fn changes_redaction_state(path: &str) -> bool {
    let path = normalize(path);
    if !(path == PREFIX || path.starts_with("/api/settings/secrets/")) {
        return false;
    }
    STATE_CHANGING
        .iter()
        .any(|(_, route)| pattern_matches(route, &path))
        || !READS.iter().any(|(_, route)| pattern_matches(route, &path))
}

/// Whether text names a Settings › Secrets route that changes state.
pub fn redaction_change(text: &str) -> bool {
    paths_with_prefix(text, PREFIX)
        .iter()
        .any(|path| changes_redaction_state(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_change_is_refused_and_every_read_is_not() {
        for (_, route) in STATE_CHANGING {
            assert!(
                changes_redaction_state(&route.replace("{id}", "x")),
                "{route}"
            );
        }
        assert!(!changes_redaction_state("/api/settings/secrets"));
        // The check is by path and ignores the method, so the two reads that share a path with a
        // change are refused too: an agent has no need to read a pattern's text or try one.
        assert!(changes_redaction_state("/api/settings/secrets/patterns/x"));
        assert!(changes_redaction_state(
            "/api/settings/secrets/patterns/test"
        ));
    }

    #[test]
    fn an_unknown_route_and_a_spelling_trick_count_as_a_change() {
        assert!(changes_redaction_state("/api/settings/secrets/new-thing"));
        assert!(changes_redaction_state(
            "/API/settings//secrets/./surfaces?x=1"
        ));
        assert!(changes_redaction_state("/api/settings/%73ecrets/surfaces"));
        assert!(!changes_redaction_state("/api/settings/secretsx"));
        assert!(!changes_redaction_state("/api/settings"));
    }

    #[test]
    fn a_command_naming_a_change_route_is_one() {
        assert!(redaction_change(
            "curl -X PUT http://127.0.0.1:1/api/settings/secrets/surfaces"
        ));
        assert!(!redaction_change(
            "curl http://127.0.0.1:1/api/settings/secrets"
        ));
    }
}
