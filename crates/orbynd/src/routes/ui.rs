//! Static UI with SPA fallback and a strict content security policy.
//! Remote raster images use the authenticated proxy, never a direct CSP grant.

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use flate2::{Compression, write::GzEncoder};
use sha2::{Digest, Sha256};

use crate::http::{ApiFailure, AppState};

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
    img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; \
    base-uri 'none'; frame-ancestors 'none'; form-action 'self'";

const SERVICE_PREFIXES: &[&str] = &["/api", "/agui", "/ws", "/mcp", "/auth"];
// Compression is CPU work. Queue it before using a blocking-pool thread.
static COMPRESS_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// HTTP q-values as thousandths; invalid values are not accepted.
fn quality(value: &str) -> Option<u16> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u16>().ok()? * 10u16.pow((3 - fraction.len()) as u32)
    };
    match whole {
        "0" => Some(fraction),
        "1" if fraction == 0 => Some(1000),
        _ => None,
    }
}

/// Select the qualities of the supported gzip and identity representations.
fn encoding_qualities(headers: &axum::http::HeaderMap) -> (u16, u16) {
    let mut gzip: Option<u16> = None;
    let mut identity: Option<u16> = None;
    let mut wildcard: Option<u16> = None;
    for value in headers.get_all(header::ACCEPT_ENCODING) {
        let Ok(value) = value.to_str() else { continue };
        for item in value.split(',') {
            let mut parts = item.split(';');
            let name = parts.next().unwrap_or_default().trim();
            let mut q = 1000;
            let mut seen_q = false;
            for part in parts {
                let Some((key, value)) = part.trim().split_once('=') else {
                    q = 0;
                    break;
                };
                if !key.trim().eq_ignore_ascii_case("q") || seen_q {
                    q = 0;
                    break;
                }
                seen_q = true;
                q = quality(value.trim()).unwrap_or(0);
            }
            let slot = if name.eq_ignore_ascii_case("gzip") {
                &mut gzip
            } else if name.eq_ignore_ascii_case("identity") {
                &mut identity
            } else if name == "*" {
                &mut wildcard
            } else {
                continue;
            };
            // Ambiguous duplicate codings cannot override an explicit refusal.
            *slot = Some(slot.map_or(q, |old| old.min(q)));
        }
    }
    (
        gzip.or(wildcard).unwrap_or(0),
        identity.or(wildcard.filter(|q| *q == 0)).unwrap_or(1000),
    )
}

fn compressible(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("html" | "js" | "mjs" | "css" | "json" | "map" | "svg" | "wasm")
    )
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Maps a URL path into `root`, refusing anything but plain components.
fn resolve(root: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(url_path)?;
    let relative = Path::new(decoded.trim_start_matches('/'));
    relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
        .then(|| root.join(relative))
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let pair = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(pair, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Classify the first decoded path component without decoding the asset name.
/// A malformed filename must not turn an asset request into an SPA navigation.
fn is_asset_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    let mut component = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = if bytes[index] == b'%' {
            let Some(pair) = path.get(index + 1..index + 3) else {
                return false;
            };
            let Ok(byte) = u8::from_str_radix(pair, 16) else {
                return false;
            };
            index += 3;
            byte
        } else {
            let byte = bytes[index];
            index += 1;
            byte
        };
        if byte == b'/' || (cfg!(windows) && byte == b'\\') {
            if !component.is_empty() {
                return component == b"assets";
            }
        } else {
            component.push(byte);
            if component.len() > b"assets".len() {
                return false;
            }
        }
    }
    component == b"assets"
}

pub async fn serve(State(state): State<AppState>, request: Request) -> Response {
    let path = request.uri().path().to_owned();
    if SERVICE_PREFIXES
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
    {
        return ApiFailure::new(StatusCode::NOT_FOUND, "not_found", "No such route.")
            .into_response();
    }
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let Some(root) = state.ui_dir.as_deref() else {
        return ApiFailure::new(StatusCode::NOT_FOUND, "no_ui", "No UI is installed.")
            .into_response();
    };
    let target = match resolve(root, &path).filter(|candidate| candidate.is_file()) {
        Some(target) => target,
        None if is_asset_path(&path) => {
            return StatusCode::NOT_FOUND.into_response();
        }
        None => root.join("index.html"),
    };
    match tokio::fs::read(&target).await {
        Ok(bytes) => {
            let (gzip_quality, identity_quality) = encoding_qualities(request.headers());
            if gzip_quality == 0 && identity_quality == 0 {
                let mut response = StatusCode::NOT_ACCEPTABLE.into_response();
                response
                    .headers_mut()
                    .insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
                return response;
            }
            let try_gzip = gzip_quality > 0
                && (identity_quality == 0
                    || (gzip_quality >= identity_quality
                        && bytes.len() >= 1024
                        && compressible(&target)));
            let (bytes, compressed) = if try_gzip {
                let Ok(permit) = COMPRESS_SLOTS.acquire().await else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                let result = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                    let encoded = encoder.write_all(&bytes).and_then(|()| encoder.finish());
                    (bytes, encoded)
                })
                .await;
                let Ok((original, encoded)) = result else {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                };
                match encoded {
                    Ok(encoded) if identity_quality == 0 || encoded.len() < original.len() => {
                        (encoded, true)
                    }
                    Ok(_) | Err(_) if identity_quality > 0 => (original, false),
                    _ => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                }
            } else {
                (bytes, false)
            };
            let digest = Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let etag = format!("\"{digest}\"");
            let matches = request
                .headers()
                .get_all(header::IF_NONE_MATCH)
                .iter()
                .filter_map(|value| value.to_str().ok())
                .flat_map(|value| value.split(','))
                .map(str::trim)
                .any(|value| value == "*" || value.strip_prefix("W/").unwrap_or(value) == etag);
            let mut response = if matches {
                StatusCode::NOT_MODIFIED.into_response()
            } else if request.method() == Method::HEAD {
                let mut response = StatusCode::OK.into_response();
                if let Ok(length) = HeaderValue::from_str(&bytes.len().to_string()) {
                    response
                        .headers_mut()
                        .insert(header::CONTENT_LENGTH, length);
                }
                response
            } else {
                bytes.into_response()
            };
            let headers = response.headers_mut();
            headers.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
            if compressed {
                headers.insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
            }
            // UI directories can contain files whose names only look content-hashed.
            // Revalidate every response until a build manifest can identify immutable assets.
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            if let Ok(value) = HeaderValue::from_str(&etag) {
                headers.insert(header::ETAG, value);
            }
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type(&target)),
            );
            headers.insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static(CSP),
            );
            headers.insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            headers.insert(
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            );
            response
        }
        Err(_) => {
            ApiFailure::new(StatusCode::NOT_FOUND, "no_ui", "No UI is installed.").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_is_refused_in_any_spelling() {
        let root = Path::new("/ui");
        assert_eq!(
            resolve(root, "/assets/a.js"),
            Some(PathBuf::from("/ui/assets/a.js"))
        );
        assert_eq!(resolve(root, "/../secret"), None);
        assert_eq!(resolve(root, "/a/%2e%2e/%2e%2e/secret"), None);
        assert_eq!(resolve(root, "/%ff"), None);
    }

    #[test]
    fn asset_classification_does_not_decode_the_filename() {
        assert!(is_asset_path("/assets/%FF.js"));
        assert!(is_asset_path("/%61ssets%2F%FF.js"));
        assert!(!is_asset_path("/assets-extra/missing.js"));
        assert_eq!(is_asset_path("/assets%5C%FF.js"), cfg!(windows));
    }

    #[test]
    fn encoding_quality_honours_explicit_refusals_and_wildcards() {
        let mut headers = axum::http::HeaderMap::new();
        assert_eq!(encoding_qualities(&headers), (0, 1000));
        for (value, expected) in [
            ("gzip", (1000, 1000)),
            ("gzip;q=0, *;q=1", (0, 1000)),
            ("*;q=0, gzip;q=0.7", (700, 0)),
            ("gzip;q=0.5, identity;q=0", (500, 0)),
            ("gzip;q=0.0000", (0, 1000)),
            ("gzip;q=0, gzip;q=1", (0, 1000)),
        ] {
            headers.insert(
                header::ACCEPT_ENCODING,
                HeaderValue::from_str(value).unwrap(),
            );
            assert_eq!(encoding_qualities(&headers), expected, "{value}");
        }
    }
}
