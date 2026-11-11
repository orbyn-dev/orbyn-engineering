//! The string shapes of `common.schema.json`. Each is a newtype that refuses a bad
//! value when it is read, so a path with `..` or a short commit never becomes a value.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

macro_rules! crew_string {
    ($(#[$meta:meta])* $name:ident, $rule:path) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, TS)]
        #[serde(transparent)]
        #[ts(export, export_to = "../../../web/src/generated/")]
        pub struct $name(String);

        impl $name {
            /// Refuses a value the schema's pattern or length rejects.
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                $rule(&value).map(|()| Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

fn all(value: &str, allowed: impl Fn(char) -> bool) -> bool {
    value.chars().all(allowed)
}

fn lower_hex(value: &str) -> bool {
    all(value, |c| matches!(c, '0'..='9' | 'a'..='f'))
}

fn word(
    value: &str,
    what: &str,
    first: fn(char) -> bool,
    rest: fn(char) -> bool,
    max: usize,
) -> Result<(), String> {
    let mut chars = value.chars();
    let ok = chars.next().is_some_and(first) && chars.all(rest) && value.chars().count() <= max;
    ok.then_some(())
        .ok_or_else(|| format!("{what} {value:?} does not match its pattern"))
}

fn hash_rule(value: &str) -> Result<(), String> {
    let ok = value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && lower_hex(hex));
    ok.then_some(())
        .ok_or_else(|| format!("hash {value:?} is not sha256: and 64 lowercase hex digits"))
}

fn id_first(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

fn id_rest(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-')
}

fn id_rule(value: &str) -> Result<(), String> {
    word(value, "id", id_first, id_rest, 128)
}

const RESERVED: [&str; 4] = ["lead", "all", "core", "person"];

fn member_rule(value: &str) -> Result<(), String> {
    id_rule(value)?;
    if RESERVED.contains(&value) {
        return Err(format!("{value:?} is a reserved word, not a member id"));
    }
    Ok(())
}

fn addressee_rule(value: &str) -> Result<(), String> {
    if matches!(value, "lead" | "all") {
        return Ok(());
    }
    member_rule(value)
}

fn token_rule(value: &str) -> Result<(), String> {
    word(
        value,
        "token",
        |c| c.is_ascii_lowercase() || c.is_ascii_digit(),
        |c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'),
        64,
    )
}

fn key_rule(value: &str) -> Result<(), String> {
    word(
        value,
        "key",
        |c| c.is_ascii_lowercase() || c.is_ascii_digit(),
        |c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'),
        128,
    )
}

fn branch_rule(value: &str) -> Result<(), String> {
    let ok = (1..=255).contains(&value.len())
        && all(value, |c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-')
        });
    ok.then_some(())
        .ok_or_else(|| format!("branch {value:?} does not match its pattern"))
}

fn commit_rule(value: &str) -> Result<(), String> {
    let ok = matches!(value.len(), 40 | 64) && lower_hex(value);
    ok.then_some(())
        .ok_or_else(|| format!("commit {value:?} is not a full lowercase hex object id"))
}

fn segment_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | '+' | ' ' | '-')
}

/// `^[A-Za-z0-9._@+ -]+(/[A-Za-z0-9._@+ -]+)*$`, no `..` segment, at most 1024 characters.
fn path_rule(value: &str) -> Result<(), String> {
    let ok = value.chars().count() <= 1024
        && value
            .split('/')
            .all(|part| !part.is_empty() && all(part, segment_char) && part != "..");
    ok.then_some(()).ok_or_else(|| {
        format!("path {value:?} must be relative, use / and have no empty or .. segment")
    })
}

/// A path, optionally ending in `/**`.
fn pattern_rule(value: &str) -> Result<(), String> {
    path_rule(value.strip_suffix("/**").unwrap_or(value))
}

fn loopback_rule(value: &str) -> Result<(), String> {
    let bad = || format!("{value:?} is not an http URL on 127.0.0.1, localhost or [::1]");
    let rest = value.strip_prefix("http://").ok_or_else(bad)?;
    let (authority, path) = rest.find('/').map_or((rest, ""), |at| rest.split_at(at));
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if host != "[" && !host.ends_with(':') => (host, Some(port)),
        _ => (authority, None),
    };
    let port_ok = port.is_none_or(|p| (1..=5).contains(&p.len()) && all(p, |c| c.is_ascii_digit()));
    let ok = matches!(host, "127.0.0.1" | "localhost" | "[::1]")
        && port_ok
        && !path.chars().any(char::is_whitespace);
    ok.then_some(()).ok_or_else(bad)
}

fn digits(value: &str, len: usize) -> Option<u32> {
    (value.len() == len && all(value, |c| c.is_ascii_digit())).then(|| value.parse().ok())?
}

fn leap(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

/// RFC 3339 `date-time`: `2026-11-12T09:00:00Z`, optional fraction, `Z` or `+HH:MM`.
fn date_time(value: &str) -> Option<()> {
    let bytes = value.as_bytes();
    if value.len() < 20 || !value.is_ascii() || !matches!(bytes[10], b'T' | b't') {
        return None;
    }
    let (year, month, day) = (
        digits(&value[0..4], 4)?,
        digits(&value[5..7], 2)?,
        digits(&value[8..10], 2)?,
    );
    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap(year) => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=days).contains(&day) {
        return None;
    }
    let (hour, minute, second) = (
        digits(&value[11..13], 2)?,
        digits(&value[14..16], 2)?,
        digits(&value[17..19], 2)?,
    );
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut rest = &value[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let end = fraction
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(fraction.len());
        if end == 0 {
            return None;
        }
        rest = &fraction[end..];
    }
    match rest {
        "Z" | "z" => Some(()),
        _ => {
            let sign = rest.strip_prefix(['+', '-'])?;
            let (h, m) = (digits(sign.get(0..2)?, 2)?, digits(sign.get(3..5)?, 2)?);
            (sign.len() == 5 && sign.as_bytes()[2] == b':' && h < 24 && m < 60).then_some(())
        }
    }
}

fn timestamp_rule(value: &str) -> Result<(), String> {
    let ok = value.len() <= 40 && date_time(value).is_some();
    ok.then_some(())
        .ok_or_else(|| format!("{value:?} is not an RFC 3339 timestamp"))
}

/// `YYYY-MM-DDTHH:MM:SSZ`: UTC and whole seconds, so two values compare as text.
fn utc_seconds_rule(value: &str) -> Result<(), String> {
    let ok = value.len() == 20 && value.ends_with('Z') && date_time(value).is_some();
    ok.then_some(())
        .ok_or_else(|| format!("{value:?} is not a UTC timestamp in whole seconds"))
}

fn text_rule(value: &str, max: usize) -> Result<(), String> {
    let length = value.chars().count();
    (1..=max)
        .contains(&length)
        .then_some(())
        .ok_or_else(|| format!("text must be 1 to {max} characters, not {length}"))
}

fn short_rule(value: &str) -> Result<(), String> {
    text_rule(value, 1000)
}

fn long_rule(value: &str) -> Result<(), String> {
    text_rule(value, 8000)
}

crew_string!(
    /// A content address: `sha256:` and 64 lowercase hex digits.
    CrewHash, hash_rule
);
crew_string!(
    /// An id the core assigns (crew, run, check). Opaque to members.
    CrewId, id_rule
);
crew_string!(
    /// A crew member. `lead`, `all`, `core` and `person` are reserved.
    CrewMember, member_rule
);
crew_string!(
    /// Who a message goes to: `lead`, `all` or a member id.
    CrewAddressee, addressee_rule
);
crew_string!(
    /// A lowercase identifier such as an engine id, a lane or a code.
    CrewToken, token_rule
);
crew_string!(
    /// The key of a board fact.
    CrewFactKey, key_rule
);
crew_string!(
    /// A git branch name as the schema allows it.
    CrewBranch, branch_rule
);
crew_string!(
    /// A full git object id in lowercase hex.
    CrewCommit, commit_rule
);
crew_string!(
    /// A repository-relative path with `/`: no leading slash, drive or `..` segment.
    CrewPath, path_rule
);
crew_string!(
    /// A path, optionally ending in `/**` to cover a tree. v0 has no other glob.
    CrewPathPattern, pattern_rule
);
crew_string!(
    /// An http URL on a loopback address. Probes never leave the machine.
    CrewLoopbackUrl, loopback_rule
);
crew_string!(
    /// An RFC 3339 timestamp.
    CrewTimestamp, timestamp_rule
);
crew_string!(
    /// A UTC timestamp in whole seconds, as a lease expiry is.
    CrewUtcSeconds, utc_seconds_rule
);
crew_string!(
    /// Text of 1 to 1000 characters.
    CrewShortText, short_rule
);
crew_string!(
    /// Text of 1 to 8000 characters.
    CrewLongText, long_rule
);

impl CrewPathPattern {
    /// The directory a `/**` pattern covers, or the file itself, and whether it is a tree.
    pub fn parts(&self) -> (&str, bool) {
        self.0
            .strip_suffix("/**")
            .map_or((&self.0, false), |dir| (dir, true))
    }

    /// Two patterns overlap when they can name the same file: equal files, or a tree
    /// and anything inside it. `crates/crew` and `crates/crew-demo` do not overlap.
    pub fn overlaps(&self, other: &Self) -> bool {
        let inside = |dir: &str, path: &str| {
            path.len() > dir.len() && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'/'
        };
        let ((a, a_tree), (b, b_tree)) = (self.parts(), other.parts());
        a == b || (a_tree && inside(a, b)) || (b_tree && inside(b, a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_follow_the_schema() {
        assert!(CrewHash::new(format!("sha256:{}", "a".repeat(64))).is_ok());
        assert!(CrewHash::new("sha256:ABC").is_err());
        assert!(CrewPath::new("crates/x/src/lib.rs").is_ok());
        for bad in ["/etc/passwd", "C:/x", "a/../b", "a//b", "a/", "..", "a/**"] {
            assert!(CrewPath::new(bad).is_err(), "{bad}");
        }
        assert!(CrewPathPattern::new("docs/**").is_ok());
        assert!(CrewPathPattern::new("**").is_err());
        assert!(CrewMember::new("lead").is_err() && CrewMember::new("worker-01").is_ok());
        assert!(CrewAddressee::new("all").is_ok() && CrewAddressee::new("core").is_err());
        assert!(CrewCommit::new("0123456789abcdef0123456789abcdef0123456").is_err());
        assert!(CrewToken::new("Claude").is_err() && CrewToken::new("claude").is_ok());
    }

    #[test]
    fn loopback_urls_stay_on_the_machine() {
        for good in [
            "http://127.0.0.1:8080/health",
            "http://localhost",
            "http://[::1]:3000/",
        ] {
            assert!(CrewLoopbackUrl::new(good).is_ok(), "{good}");
        }
        for bad in [
            "https://127.0.0.1",
            "http://example.com",
            "http://127.0.0.1.evil.com/",
            "http://localhost:/x",
            "http://127.0.0.1:123456",
        ] {
            assert!(CrewLoopbackUrl::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn timestamps_are_real_dates() {
        assert!(CrewTimestamp::new("2026-11-12T09:00:00Z").is_ok());
        assert!(CrewTimestamp::new("2026-11-12T09:00:00.5+01:00").is_ok());
        assert!(CrewTimestamp::new("2026-02-30T09:00:00Z").is_err());
        assert!(CrewTimestamp::new("2026-11-12 09:00:00Z").is_err());
        assert!(CrewUtcSeconds::new("2026-11-12T09:00:00Z").is_ok());
        assert!(CrewUtcSeconds::new("2026-11-12T09:00:00.5Z").is_err());
        assert!(CrewUtcSeconds::new("2026-11-12T09:00:00+00:00").is_err());
    }

    #[test]
    fn overlap_is_by_whole_segments() {
        let p = |s: &str| CrewPathPattern::new(s).unwrap();
        assert!(p("crates/crew/**").overlaps(&p("crates/crew/src/lib.rs")));
        assert!(!p("crates/crew/**").overlaps(&p("crates/crew-demo/**")));
        assert!(p("a/b").overlaps(&p("a/b")));
        assert!(!p("a/b").overlaps(&p("a/c")));
    }
}
