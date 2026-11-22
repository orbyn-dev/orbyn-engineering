//! Pre-compressed static representations.
//!
//! The UI build writes a brotli and a gzip sibling beside every compressible file
//! (`web/scripts/precompress.mjs`). Serving one of those costs no per-request CPU
//! and gives the best ratio we can compute offline, which matters most for the
//! large script bundle. A client that accepts neither, an asset whose type is
//! already compressed, and a directory that was never pre-compressed all keep the
//! identity response the service has always sent.

use std::path::{Path, PathBuf};

use axum::http::{HeaderMap, header};

/// Full quality, in the thousandths of RFC 9110's `qvalue`.
const QUALITY_MAX: u16 = 1000;

struct Encoding {
    /// The `Content-Encoding` value, and the sibling's file suffix.
    name: &'static str,
    suffix: &'static str,
}

/// Preference order: brotli is smaller than gzip at every level we build, so it wins
/// when a client accepts both equally.
const ENCODINGS: [Encoding; 2] = [
    Encoding {
        name: "br",
        suffix: "br",
    },
    Encoding {
        name: "gzip",
        suffix: "gz",
    },
];

/// The bytes to serve and the coding they are already in.
pub struct Served {
    pub bytes: Vec<u8>,
    pub encoding: &'static str,
}

/// Types whose encoding is worth a sibling. Everything else is already compressed
/// (images, fonts) or too small to gain, so it is never encoded.
pub fn compressible(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    [
        "html", "js", "mjs", "css", "json", "map", "svg", "txt", "wasm",
    ]
    .iter()
    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

/// A compressed sibling is a transport artifact, never a resource: a request that
/// names one must fall through instead of receiving the coded bytes as a file.
pub fn is_sibling(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ENCODINGS
                .iter()
                .any(|encoding| extension.eq_ignore_ascii_case(encoding.suffix))
        })
}

/// Picks the sibling a client accepts best, if any beats the identity response.
pub async fn negotiate(target: &Path, headers: &HeaderMap) -> Option<Served> {
    if !compressible(target) {
        return None;
    }
    let identity = identity_quality(headers);
    let mut best: Option<(u16, &'static Encoding, PathBuf)> = None;
    for encoding in &ENCODINGS {
        // An encoding the client did not ask for is not acceptable, and one it ranks
        // below identity is not an improvement.
        let quality = stated_quality(headers, encoding.name).unwrap_or(0);
        if quality == 0 || quality < identity {
            continue;
        }
        let sibling = sibling(target, encoding.suffix);
        if !sibling.is_file() {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(current, _, _)| quality > *current)
        {
            best = Some((quality, encoding, sibling));
        }
    }
    let (_, encoding, sibling) = best?;
    let bytes = tokio::fs::read(&sibling).await.ok()?;
    Some(Served {
        bytes,
        encoding: encoding.name,
    })
}

fn sibling(target: &Path, suffix: &str) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

/// The quality the client states for one coding, explicitly or through `*`.
fn stated_quality(headers: &HeaderMap, coding: &str) -> Option<u16> {
    let mut stated: Option<u16> = None;
    let mut wildcard: Option<u16> = None;
    for value in headers.get_all(header::ACCEPT_ENCODING) {
        let Ok(value) = value.to_str() else {
            continue;
        };
        for item in value.split(',') {
            let mut parameters = item.split(';');
            let name = parameters.next().unwrap_or_default().trim();
            let mut quality = QUALITY_MAX;
            let mut usable = true;
            for parameter in parameters {
                let Some((key, value)) = parameter.trim().split_once('=') else {
                    continue;
                };
                if key.trim().eq_ignore_ascii_case("q") {
                    // A malformed quality makes the entry count for nothing, never
                    // for everything.
                    match qvalue(value) {
                        Some(parsed) => quality = parsed,
                        None => usable = false,
                    }
                }
            }
            if !usable {
                continue;
            }
            if name.eq_ignore_ascii_case(coding) {
                stated = Some(stated.map_or(quality, |current| current.max(quality)));
            } else if name == "*" {
                wildcard = Some(wildcard.map_or(quality, |current| current.max(quality)));
            }
        }
    }
    stated.or(wildcard)
}

/// `identity` is acceptable unless the client excludes it, which is why a request
/// with no `Accept-Encoding` never selects a sibling.
fn identity_quality(headers: &HeaderMap) -> u16 {
    if !headers.contains_key(header::ACCEPT_ENCODING) {
        return QUALITY_MAX;
    }
    stated_quality(headers, "identity").unwrap_or(QUALITY_MAX)
}

/// Parses RFC 9110's `qvalue` (0 to 1, at most three decimals) as thousandths.
fn qvalue(raw: &str) -> Option<u16> {
    let raw = raw.trim();
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    if whole.len() != 1
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole = u16::from(whole.as_bytes()[0] - b'0');
    if whole > 1 {
        return None;
    }
    let mut scale = 100;
    let mut value = 0;
    for byte in fraction.bytes() {
        value += u16::from(byte - b'0') * scale;
        scale /= 10;
    }
    Some(whole * QUALITY_MAX + value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    fn headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_str(value).unwrap(),
        );
        headers
    }

    #[test]
    fn qualities_follow_the_accept_encoding_grammar() {
        assert_eq!(stated_quality(&headers("gzip, br"), "br"), Some(1000));
        assert_eq!(stated_quality(&headers("br;q=0.5"), "br"), Some(500));
        assert_eq!(stated_quality(&headers("br;q=0"), "br"), Some(0));
        // An unlisted coding without a wildcard was never asked for.
        assert_eq!(stated_quality(&headers("gzip"), "br"), None);
        assert_eq!(stated_quality(&headers("*;q=0.1"), "br"), Some(100));
        assert_eq!(stated_quality(&headers("*;q=0"), "br"), Some(0));
        // Repeated entries keep the best one.
        assert_eq!(stated_quality(&headers("br;q=0, br;q=1"), "br"), Some(1000));
        // Garbage is not consent.
        assert_eq!(stated_quality(&headers("br;q=2"), "br"), None);
        assert_eq!(stated_quality(&headers("br;q=abc"), "br"), None);
    }

    #[test]
    fn a_client_that_does_not_ask_for_an_encoding_gets_identity() {
        // No `Accept-Encoding` at all: identity is the only acceptable coding.
        assert_eq!(identity_quality(&HeaderMap::new()), 1000);
        assert_eq!(stated_quality(&HeaderMap::new(), "br"), None);
        // `identity` stays acceptable beside another coding...
        assert_eq!(identity_quality(&headers("br")), 1000);
        // ...unless it is excluded, explicitly or through the wildcard.
        assert_eq!(identity_quality(&headers("identity;q=0")), 0);
        assert_eq!(identity_quality(&headers("*;q=0")), 0);
        assert_eq!(identity_quality(&headers("br;q=0.5, identity;q=0.7")), 700);
    }

    #[test]
    fn only_types_worth_encoding_are_offered_a_sibling() {
        for path in [
            "assets/index-abc.js",
            "assets/index-abc.mjs",
            "assets/index-abc.css",
            "index.html",
            "assets/data.json",
            "assets/icon.svg",
            "assets/font.wasm",
        ] {
            assert!(compressible(Path::new(path)), "{path}");
        }
        for path in [
            "assets/photo.png",
            "assets/font.woff2",
            "assets/font.woff",
            "favicon.ico",
            "assets/image.jpg",
            "assets/archive.zip",
            "assets/blob",
        ] {
            assert!(!compressible(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn a_compressed_sibling_is_not_a_resource_of_its_own() {
        for path in [
            "assets/index-abc.js.br",
            "assets/index-abc.js.gz",
            "shell.HTML.BR",
        ] {
            assert!(is_sibling(Path::new(path)), "{path}");
        }
        for path in ["assets/index-abc.js", "index.html", "assets/data.json"] {
            assert!(!is_sibling(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn a_sibling_sits_beside_its_file() {
        assert_eq!(
            sibling(Path::new("/ui/assets/index.js"), "br"),
            PathBuf::from("/ui/assets/index.js.br")
        );
    }
}
