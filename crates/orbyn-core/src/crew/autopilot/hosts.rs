//! The hosts a repository allows: `allowed_hosts` in `.orbyn/crew.json`, read at the commit the
//! work started from so a worker cannot add its own (N3, N7). A missing file or key allows nothing
//! but loopback, and so does a file that cannot be read: not knowing is never a reason to allow.

use std::path::Path;

use serde::Deserialize;

use crate::crew::RULES_FILE;
use crate::source_control::git::{git_hardened as git, text};

#[derive(Default, Deserialize)]
struct File {
    #[serde(default)]
    allowed_hosts: Vec<String>,
}

fn valid(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '*'))
        && (!host.contains('*') || host.starts_with("*.") && host[2..].chars().all(|c| c != '*'))
}

/// The allowed hosts at `base`, lowercase. Entries that are not hostnames are dropped.
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
    if shown.len() > 64 * 1024 {
        return Vec::new();
    }
    serde_json::from_str::<File>(&text(&shown))
        .map(|file| {
            file.allowed_hosts
                .into_iter()
                .map(|h| h.to_lowercase())
                .filter(|h| valid(h))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::valid;

    #[test]
    fn only_hostnames_and_leading_wildcards_are_kept() {
        for ok in ["github.com", "*.example.org", "a-b.example"] {
            assert!(valid(ok), "{ok}");
        }
        for bad in ["", "a b", "https://x.com", "*", "a.*.com", "x/y", "*.*.com"] {
            assert!(!valid(bad), "{bad}");
        }
    }
}
