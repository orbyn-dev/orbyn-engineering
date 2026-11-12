//! The glob of a data class (version 1). A pattern is `/`-separated segments. `*` stands for any run of characters inside one
//! segment, and a segment that is exactly `**` stands for any number of segments, none included. Nothing else is special:
//! `?`, `[`, `]`, `{` and `}` are refused, so a pattern means one thing to every reader. A pattern with a backslash, a leading
//! slash, a drive letter, an empty segment or a `.` or `..` segment is invalid.
//!
//! Matching ignores case and Unicode normalisation: both sides are put in NFC, lowercased and put in NFC again. On a case-folding
//! file system (APFS, NTFS) two spellings are one file, and folding can only make a pattern match more, which tightens a rule.

use icu_normalizer::ComposingNormalizerBorrowed;

/// Longest pattern, in characters.
pub const GLOB_MAX: usize = 200;

/// NFC, lowercase, NFC.
pub fn fold(text: &str) -> String {
    let nfc = ComposingNormalizerBorrowed::new_nfc();
    nfc.normalize(&nfc.normalize(text).to_lowercase())
        .into_owned()
}

/// Why a repository path is not usable as one (a path a person types into the tester, or a symlink's target).
pub fn path_problem(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("is empty");
    }
    if path.contains('\\') {
        return Some("has a backslash; paths use /");
    }
    if path.starts_with('/') {
        return Some("starts with /; paths are relative to the repository");
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Some("starts with a drive letter");
    }
    if path.chars().any(char::is_control) {
        return Some("has a control character");
    }
    for segment in path.split('/') {
        match segment {
            "" => return Some("has an empty part (a double or trailing /)"),
            "." | ".." => return Some("has a . or .. part"),
            _ => {}
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    /// `**`.
    Deep,
    /// Literal pieces around `*`: `a*b*` is `["a", "b", ""]`.
    One(Vec<Vec<char>>),
}

/// A parsed pattern, kept with the text it was written as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glob {
    text: String,
    segments: Vec<Segment>,
}

impl Glob {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.chars().count() > GLOB_MAX {
            return Err(format!("is longer than {GLOB_MAX} characters"));
        }
        if let Some(why) = path_problem(text) {
            return Err(why.to_owned());
        }
        if let Some(bad) = text
            .chars()
            .find(|c| matches!(c, '?' | '[' | ']' | '{' | '}'))
        {
            return Err(format!(
                "uses {bad}, which version 1 patterns do not support (only * and **)"
            ));
        }
        let mut segments = Vec::new();
        for part in fold(text).split('/') {
            if part == "**" {
                // `a/**/**` and `a/**` mean the same: one is enough.
                if segments.last() != Some(&Segment::Deep) {
                    segments.push(Segment::Deep);
                }
            } else if part.contains("**") {
                return Err("uses ** inside a name; ** is a whole part (a/**/b)".to_owned());
            } else {
                segments.push(Segment::One(
                    part.split('*').map(|p| p.chars().collect()).collect(),
                ));
            }
        }
        Ok(Self {
            text: text.to_owned(),
            segments,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the repository path matches.
    pub fn matches(&self, path: &str) -> bool {
        self.walk(&fold(path))[self.segments.len()]
    }

    /// Whether some path under the directory `dir` (or `dir` itself) could match: the pattern's parts are walked against the
    /// directory's, and whatever the pattern still asks for after them is something the lease lets a worker create. This
    /// widens on purpose: a lease on `src/**` can make `src/auth/x`, so `**/auth/**` may match under it.
    pub fn may_match_under(&self, dir: &str) -> bool {
        self.walk(&fold(dir)).iter().any(|on| *on)
    }

    /// Which prefixes of the pattern the folded path's parts have matched: entry `i` says the first `i` segments have.
    fn walk(&self, folded: &str) -> Vec<bool> {
        let mut here = vec![false; self.segments.len() + 1];
        here[0] = true;
        self.close(&mut here);
        for part in folded.split('/') {
            let mut next = vec![false; here.len()];
            for (at, on) in here.iter().enumerate() {
                if !*on || at == self.segments.len() {
                    continue;
                }
                match &self.segments[at] {
                    Segment::Deep => next[at] = true,
                    Segment::One(pieces) if one(pieces, part) => next[at + 1] = true,
                    Segment::One(_) => {}
                }
            }
            self.close(&mut next);
            here = next;
        }
        here
    }

    /// A `**` may match nothing, so reaching it is also reaching what follows.
    fn close(&self, here: &mut [bool]) {
        for at in 0..self.segments.len() {
            if here[at] && self.segments[at] == Segment::Deep {
                here[at + 1] = true;
            }
        }
    }
}

/// One name against the pieces between the `*`s of a segment.
fn one(pieces: &[Vec<char>], name: &str) -> bool {
    let name: Vec<char> = name.chars().collect();
    let (first, rest) = pieces.split_first().expect("a split has a piece");
    if rest.is_empty() {
        return *first == name;
    }
    let last = rest.last().expect("not empty");
    if name.len() < first.len() + last.len()
        || !name.starts_with(first)
        || !name.ends_with(last.as_slice())
    {
        return false;
    }
    let mut at = first.len();
    let end = name.len() - last.len();
    for piece in &rest[..rest.len() - 1] {
        match find(&name[at..end], piece) {
            Some(found) => at += found + piece.len(),
            None => return false,
        }
    }
    true
}

fn find(hay: &[char], piece: &[char]) -> Option<usize> {
    if piece.is_empty() {
        return Some(0);
    }
    hay.windows(piece.len()).position(|w| w == piece)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(text: &str) -> Glob {
        Glob::parse(text).unwrap_or_else(|why| panic!("{text}: {why}"))
    }

    #[test]
    fn star_stays_in_one_segment_and_double_star_crosses() {
        let (any, one_level, tail) = (g("**/auth/**"), g("src/*.rs"), g("crates/keys/**"));
        assert!(any.matches("auth/login.rs") && any.matches("a/b/auth/c/d.rs"));
        assert!(!any.matches("src/authz/login.rs"));
        assert!(one_level.matches("src/lib.rs") && !one_level.matches("src/a/lib.rs"));
        assert!(tail.matches("crates/keys/src/lib.rs") && !tail.matches("crates/keysmith/a"));
        assert!(g("a*b").matches("ab") && g("a*b").matches("axxb") && !g("a*b").matches("axbx"));
        assert!(g("*.env").matches(".env") && g("*.env").matches("prod.env"));
        assert!(
            g("a*b*c").matches("abc") && g("a*b*c").matches("aXbYc") && !g("a*b*c").matches("acb")
        );
    }

    #[test]
    fn case_and_normalisation_are_folded() {
        let keys = g("crates/Keys/**");
        assert!(keys.matches("CRATES/keys/lib.rs"));
        // "e" + combining acute is the same name as the precomposed letter.
        assert!(g("docs/caf\u{e9}/**").matches("docs/cafe\u{301}/x"));
        assert!(g("docs/cafe\u{301}/**").matches("DOCS/CAF\u{c9}/x"));
    }

    #[test]
    fn what_a_pattern_cannot_say_is_refused() {
        for bad in [
            "", "/a", "a/", "a//b", "a/../b", "..", "./a", "a\\b", "C:/a", "c:a", "a/?.rs",
            "a/[x]", "a/{b,c}", "a/**b", "a/b**", "a\u{0}b",
        ] {
            assert!(Glob::parse(bad).is_err(), "{bad:?}");
        }
        assert!(Glob::parse(&"a/".repeat(101)).is_err());
        assert!(Glob::parse("**").is_ok() && Glob::parse(".github/**").is_ok());
    }

    #[test]
    fn a_lease_may_hold_a_match_when_the_walk_can_get_there() {
        let keys = g("crates/keys/**");
        assert!(keys.may_match_under("crates") && keys.may_match_under("crates/keys/src"));
        assert!(!keys.may_match_under("docs") && !keys.may_match_under("crates/other"));
        // A pattern that floats can be made under any directory.
        assert!(g("**/auth/**").may_match_under("src"));
        assert!(!g("src/*.rs").may_match_under("src/deep"));
        assert!(g("src/*.rs").may_match_under("src"));
        assert!(g("src/*.rs").may_match_under("SRC"));
    }
}
