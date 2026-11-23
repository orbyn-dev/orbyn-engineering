//! The hosts a repository allows: `allowed_hosts` in `.orbyn/crew.json`, read at the commit the
//! work started from so a worker cannot add its own (N3, N7). A missing file or key allows nothing
//! but loopback, and so does a file that cannot be read: not knowing is never a reason to allow.

use std::net::IpAddr;
use std::path::Path;

use serde::Deserialize;

use crate::crew::RULES_FILE;
use crate::source_control::git::git_hardened as git;

#[derive(Default, Deserialize)]
struct File {
    #[serde(default)]
    allowed_hosts: Vec<String>,
}

/// Canonical spelling only: this does not resolve a name or establish who owns its destination.
pub(super) fn normalize_host(host: &str) -> Option<String> {
    // Brackets belong only to an IPv6 literal, never to a name, port or another bracket pair.
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return inner
            .parse::<std::net::Ipv6Addr>()
            .ok()
            .map(|ip| ip.to_string());
    }
    if host.contains(':') {
        return host
            .parse::<std::net::Ipv6Addr>()
            .ok()
            .map(|ip| ip.to_string());
    }
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() || host.len() > 253 || !host.is_ascii() {
        return None;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip.to_string());
    }
    // URL clients can read short, integer, octal or hexadecimal IPv4 spellings differently
    // from DNS. Never fall back to a name when its final label looks like an address number.
    let last = host.rsplit('.').next()?;
    let number = last.bytes().all(|c| c.is_ascii_digit())
        || last
            .strip_prefix("0x")
            .or_else(|| last.strip_prefix("0X"))
            .is_some_and(|digits| digits.bytes().all(|c| c.is_ascii_hexdigit()));
    if number
        || !host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
    {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

pub(super) fn normalize_entry(entry: &str) -> Option<String> {
    match entry.strip_prefix("*.") {
        Some(suffix) => {
            let suffix = normalize_host(suffix)?;
            if suffix.parse::<IpAddr>().is_ok() || suffix.len() + 2 > 253 {
                return None;
            }
            Some(format!("*.{suffix}"))
        }
        None => normalize_host(entry),
    }
}

/// Nonblocking diagnostics only. Admission still uses `File` and `normalize_entry` below.
/// Never quote a submitted value: this output is served to the repository's rules view.
pub(in crate::crew) fn allowed_host_notes(value: Option<&serde_json::Value>) -> Vec<String> {
    const NOTES_MAX: usize = 16;
    let Some(value) = value else {
        return Vec::new();
    };
    let Some(entries) = value.as_array() else {
        return vec![
            "allowed_hosts must be an array of strings; no configured hosts are allowed."
                .to_owned(),
        ];
    };
    // Vec<String> deserialization refuses the whole list, even when other entries are valid.
    if let Some(index) = entries.iter().position(|entry| !entry.is_string()) {
        return vec![format!(
            "allowed_hosts[{index}] must be a string; no configured hosts are allowed."
        )];
    }
    let mut notes: Vec<String> = entries.iter().enumerate().filter_map(|(index, entry)| {
        match normalize_entry(entry.as_str()?) {
            None => Some(format!("allowed_hosts[{index}] is not a valid host entry and is ignored.")),
            Some(entry) if entry.strip_prefix("*.").is_some_and(|base| !base.contains('.')) => Some(format!("allowed_hosts[{index}] uses a wildcard with a one-label base. It remains allowed for the base host and all descendants. No public-suffix check is performed.")),
            Some(_) => None,
        }
    }).take(NOTES_MAX + 1).collect();
    if notes.len() > NOTES_MAX {
        notes[NOTES_MAX] = "More allowed_hosts notes were omitted. Check the remaining entries in .orbyn/crew.json.".to_owned();
    }
    notes
}

fn parse_allowed_hosts(shown: &[u8]) -> Vec<String> {
    if shown.len() > 64 * 1024 {
        return Vec::new();
    }
    serde_json::from_slice::<File>(shown)
        .map(|file| {
            file.allowed_hosts
                .iter()
                .filter_map(|h| normalize_entry(h))
                .collect()
        })
        .unwrap_or_default()
}

/// The allowed hosts at `base`, canonicalized. Malformed entries are dropped individually.
pub async fn read_allowed_hosts(root: &Path, base: Option<&str>) -> Vec<String> {
    let Some(base) = base else { return Vec::new() };
    let Ok(listed) = git(root, &["ls-tree", base, "--", RULES_FILE]).await else {
        return Vec::new();
    };
    if listed.is_empty() {
        return Vec::new();
    }
    let Ok(shown) = git(root, &["show", &format!("{base}:{RULES_FILE}")]).await else {
        return Vec::new();
    };
    parse_allowed_hosts(&shown)
}

#[cfg(test)]
mod tests {
    use super::{normalize_entry, parse_allowed_hosts};

    fn valid(host: &str) -> bool {
        normalize_entry(host).is_some()
    }

    #[test]
    fn only_hostnames_and_leading_wildcards_are_kept() {
        for ok in ["github.com", "*.example.org", "a-b.example"] {
            assert!(valid(ok), "{ok}");
        }
        for bad in ["", "a b", "https://x.com", "*", "a.*.com", "x/y", "*.*.com"] {
            assert!(!valid(bad), "{bad}");
        }
    }

    #[test]
    fn malformed_labels_and_wildcard_addresses_are_not_hosts() {
        for bad in [
            ".",
            "..",
            ".example.org",
            "example..org",
            "example.org..",
            "-collector.internal",
            "collector-.internal",
            "*.",
            "*..internal",
            "*.127.0.0.1",
            "*.10.0.0.1",
            "256.1.2.3",
            "10.1.2",
            "01.2.3.4",
        ] {
            assert!(!valid(bad), "{bad}");
        }
        assert!(!valid(&format!("{}.internal", "a".repeat(64))));
        assert!(valid(&format!("{}.internal", "a".repeat(63))));
        assert!(valid(
            &[
                "a".repeat(63),
                "b".repeat(63),
                "c".repeat(63),
                "d".repeat(61)
            ]
            .join(".")
        ));
        assert!(!valid(
            &[
                "a".repeat(63),
                "b".repeat(63),
                "c".repeat(63),
                "d".repeat(62)
            ]
            .join(".")
        ));
        let wildcard_limit = [
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(59),
        ]
        .join(".");
        assert!(valid(&format!("*.{wildcard_limit}")));
        assert!(!valid(&format!("*.{wildcard_limit}d")));
    }

    #[test]
    fn explicit_local_private_and_internal_hosts_remain_valid() {
        for ok in [
            "collector",
            "collector.internal",
            "*.internal",
            "10.0.0.7",
            "192.168.1.2",
            "172.16.0.1",
            "fd00::7",
            "[fd00::7]",
            "::1",
        ] {
            assert!(valid(ok), "{ok}");
        }
    }

    #[test]
    fn ambiguous_addresses_ports_and_brackets_fail_closed() {
        for bad in [
            "127.1",
            "2130706433",
            "0x7f000001",
            "0X7F000001",
            "0177.0.0.1",
            "0x",
            "0X",
            "0x.",
            "collector.0x",
            "*.0x",
            "*.0X.",
            "0x7f.0.0.1",
            "collector.123",
            "collector:443",
            "[collector]",
            "[10.0.0.7]",
            "[[::1]]",
            "[fd00::7]:443",
            "fd00::7%eth0",
            "[::1",
            "::1.",
        ] {
            assert!(!valid(bad), "{bad}");
        }
        assert!(valid("123.collector.internal"));
    }

    #[test]
    fn config_canonicalizes_valid_entries_and_drops_only_invalid_entries() {
        let bytes = br#"{"allowed_hosts":["COLLECTOR.Internal.","*.","*.Logs.Internal.","10.0.0.7","[fd00:0:0:0:0:0:0:7]","*.10.0.0.7","collector:443"]}"#;
        assert_eq!(
            parse_allowed_hosts(bytes),
            [
                "collector.internal",
                "*.logs.internal",
                "10.0.0.7",
                "fd00::7",
            ]
        );
        for invalid in [
            &b"not json"[..],
            &b"{}"[..],
            &b"{\"allowed_hosts\":null}"[..],
            &b"{\"allowed_hosts\":[\"collector\",7]}"[..],
            &b"{\"allowed_hosts\":[\"col\xfflector\"]}"[..],
        ] {
            assert!(parse_allowed_hosts(invalid).is_empty());
        }
        let mut bounded = br#"{"allowed_hosts":["collector"]}"#.to_vec();
        bounded.resize(64 * 1024, b' ');
        assert_eq!(parse_allowed_hosts(&bounded), ["collector"]);
        bounded.push(b' ');
        assert!(parse_allowed_hosts(&bounded).is_empty());
    }

    #[test]
    fn diagnostics_must_not_change_existing_host_admission() {
        let hosts = parse_allowed_hosts(br#"{"allowed_hosts":["collector","bad:443","*.COM.","*.co.uk","*.internal","10.0.0.7","[fd00::7]"]}"#);
        assert_eq!(
            hosts,
            [
                "collector",
                "*.com",
                "*.co.uk",
                "*.internal",
                "10.0.0.7",
                "fd00::7"
            ]
        );
        assert!(parse_allowed_hosts(br#"{"allowed_hosts":["collector",7]}"#).is_empty());
        for host in [
            "collector",
            "com",
            "a.com",
            "a.b.com",
            "a.co.uk",
            "a.internal",
            "10.0.0.7",
            "fd00::7",
            "127.0.0.1",
        ] {
            assert!(super::super::shell::host_allowed(host, &hosts), "{host}");
        }
        assert!(!super::super::shell::host_allowed(
            "unlisted.example",
            &hosts
        ));
    }
}
