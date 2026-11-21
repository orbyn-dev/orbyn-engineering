//! What a clone source may be, and the folder names that come from untrusted text (docs/multi-repo.md,
//! "Clone and path input").

use crate::error::{CoreError, Result};

const MAX_SOURCE: usize = 2048;

fn refused(why: &str) -> CoreError {
    CoreError::CloneRefused(format!("That source cannot be cloned: {why}."))
}

/// The one check every clone source passes: `https://`, `ssh://` or scp-style `user@host:path`, and nothing else.
/// Anything Git would read as an option, a helper, a descriptor, a local path or a login with a password is refused.
pub(crate) fn check_source(source: &str) -> Result<&str> {
    let text = source.trim();
    if text.is_empty() || text.len() > MAX_SOURCE {
        return Err(refused("it is empty or too long"));
    }
    if text
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
    {
        return Err(refused(
            "it has a space, a control character or a backslash",
        ));
    }
    if text.starts_with('-') {
        return Err(refused("it starts with a dash"));
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("::") || lower.starts_with("file:") || lower.starts_with("fd:") {
        return Err(refused("it names a Git helper or a local file"));
    }
    if let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("ssh://"))
    {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let (user, host) = authority.rsplit_once('@').unwrap_or(("", authority));
        let host = host.rsplit_once(':').map_or(host, |(host, port)| {
            if port.bytes().all(|b| b.is_ascii_digit()) {
                host
            } else {
                ""
            }
        });
        if user.contains(':') {
            return Err(refused("it holds a password"));
        }
        if host.is_empty() || !host_ok(host) || !rest.contains('/') {
            return Err(refused("it has no host or no path"));
        }
        return Ok(text);
    }
    if lower.contains("://") {
        return Err(refused("only https, ssh and user@host:path are allowed"));
    }
    // scp-style: user@host:path, with no scheme and a path that is not another scheme.
    if let Some((user, rest)) = text.split_once('@')
        && let Some((host, path)) = rest.split_once(':')
        && !user.is_empty()
        && user
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && host_ok(host)
        && !path.is_empty()
        && !path.starts_with("//")
        && !path.starts_with('-')
    {
        return Ok(text);
    }
    Err(refused("only https, ssh and user@host:path are allowed"))
}

fn host_ok(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with(['-', '.'])
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'[' | b']' | b':'))
}

/// A safe folder name from untrusted text: letters, digits, `-`, `_`, `.`; never a leading dot; at most 64.
pub(crate) fn folder_name(text: &str) -> Option<String> {
    let mapped: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name: String = mapped.trim_matches(['-', '.']).chars().take(64).collect();
    (!name.is_empty() && name != "." && name != "..").then_some(name)
}

/// The last path segment of a source, without `.git`.
pub(crate) fn name_of_source(source: &str) -> Option<String> {
    let trimmed = source.trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next()?;
    folder_name(last.strip_suffix(".git").unwrap_or(last))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_ssh_and_scp_sources_pass() {
        for ok in [
            "https://github.com/a/b.git",
            "https://github.com/a/b",
            "ssh://git@github.com/a/b.git",
            "ssh://git@host.example:2222/a/b.git",
            "git@github.com:a/b.git",
            "https://user@host.example/a/b",
        ] {
            assert!(check_source(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "-x",
            "--upload-pack=touch /tmp/x",
            "ext::sh -c id",
            "EXT::sh",
            "fd::1",
            "file:///etc",
            "FILE:///etc",
            "/tmp/repo",
            "./repo",
            "C:\\\\repo",
            "../repo",
            "https://u:p@host/a/b",
            "ssh://u:p@host/a/b",
            "https://",
            "https:///a/b",
            "https://host",
            "http://host/a",
            "git://host/a",
            "https://host/a\\nb",
            "https://host/a b",
            "a@b:-x",
            "https://-host/a",
        ] {
            assert!(check_source(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_folder_name_is_safe_whatever_the_text() {
        assert_eq!(
            name_of_source("https://github.com/a/b.git").as_deref(),
            Some("b")
        );
        assert_eq!(
            name_of_source("git@github.com:a/b.git").as_deref(),
            Some("b")
        );
        assert_eq!(folder_name("..").as_deref(), None);
        assert_eq!(folder_name("../../etc").as_deref(), Some("etc"));
        assert_eq!(folder_name(".hidden").as_deref(), Some("hidden"));
        assert_eq!(folder_name("a/b c").as_deref(), Some("a-b-c"));
        assert_eq!(folder_name(&"x".repeat(100)).unwrap().len(), 64);
    }
}
