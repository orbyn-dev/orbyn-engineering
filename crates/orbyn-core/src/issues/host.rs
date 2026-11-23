//! A tracker host as a person types it: https only, nothing a request could be steered with.

use crate::error::{CoreError, Result};

fn refuse(why: &str) -> CoreError {
    CoreError::Invalid(format!("That host is not allowed: {why}."))
}

/// The host normalized (lowercase host, no trailing slash), or why it is refused. It must be `https://`,
/// with no user info, query or fragment, no spaces or control characters, and a path of plain characters.
pub fn validate_host(input: &str) -> Result<String> {
    let text = input.trim();
    if text.chars().any(|c| c.is_whitespace() || c.is_control()) || text.len() > 253 {
        return Err(refuse(
            "it has spaces or control characters, or is too long",
        ));
    }
    let rest = text
        .strip_prefix("https://")
        .ok_or_else(|| refuse("it must start with https://"))?;
    if rest.contains(['?', '#', '\\']) {
        return Err(refuse("it has a query or fragment"));
    }
    let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    if authority.contains('@') {
        return Err(refuse("it has a user name or password in it"));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if host.is_empty()
        || host.starts_with(['.', '-'])
        || host.ends_with(['.', '-'])
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        || !host.contains('.') && host != "localhost"
    {
        return Err(refuse("it is not a host name"));
    }
    if let Some(port) = port
        && (port.is_empty() || port.len() > 5 || !port.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(refuse("its port is not a number"));
    }
    let path = path.trim_matches('/');
    if path.split('/').any(|part| part == ".." || part == ".")
        || !path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._~/-".contains(c))
    {
        return Err(refuse("its path has unusual characters"));
    }
    let mut out = format!("https://{}", authority.to_ascii_lowercase());
    if !path.is_empty() {
        out.push('/');
        out.push_str(path);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_https_host_is_normalized() {
        assert_eq!(
            validate_host("https://GitLab.Example.com/").unwrap(),
            "https://gitlab.example.com"
        );
        assert_eq!(
            validate_host(" https://code.example.org:8443/forge/ ").unwrap(),
            "https://code.example.org:8443/forge"
        );
        assert_eq!(
            validate_host("https://dev.azure.com/acme").unwrap(),
            "https://dev.azure.com/acme"
        );
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in [
            "http://gitlab.com",
            "gitlab.com",
            "https://user:pw@gitlab.com",
            "https://gitlab.com?x=1",
            "https://gitlab.com/#frag",
            "https://git lab.com",
            "https://gitlab.com/a/../b",
            "https://gitlab.com/a b",
            "https://-bad.com",
            "https://nodots",
            "https://gitlab.com:abc",
            "https://gitlab.com\\evil.com",
            "ftp://gitlab.com",
            "https://",
            "",
        ] {
            assert!(validate_host(bad).is_err(), "{bad:?} must be refused");
        }
    }
}
