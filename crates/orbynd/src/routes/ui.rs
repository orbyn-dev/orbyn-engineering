//! Static UI with SPA fallback and a strict content security policy.
//! Remote raster images use the authenticated proxy, never a direct CSP grant.

use std::path::{Component, Path, PathBuf};

use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::http::{ApiFailure, AppState};

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
    img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; \
    base-uri 'none'; frame-ancestors 'none'; form-action 'self'";

const SERVICE_PREFIXES: &[&str] = &["/api", "/agui", "/ws", "/mcp", "/auth"];

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

/// Build output whose name carries its content hash can never change under that
/// URL, so a client may keep it for a year without asking again.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// `index.html` and every other file keep their URL across a deploy, so a client
/// must revalidate them; otherwise a new shell would never reach a stale tab.
const REVALIDATE: &str = "no-cache";

/// Vite writes build output as `assets/<name>-<hash>.<ext>`; only a hashed name
/// under `assets/` is safe to pin, because anything else can change in place.
fn cache_control(relative: &Path) -> &'static str {
    let under_assets = relative
        .components()
        .next()
        .is_some_and(|part| part.as_os_str() == "assets");
    let hashed = relative
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.rsplit_once('-'))
        .is_some_and(|(_, token)| is_content_hash(token));
    if under_assets && hashed {
        IMMUTABLE
    } else {
        REVALIDATE
    }
}

/// Vite's default content hash is eight base64url characters; it may be longer.
fn is_content_hash(token: &str) -> bool {
    (8..=32).contains(&token.len())
        && token.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
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
    let target = resolve(root, &path)
        .filter(|candidate| candidate.is_file())
        .unwrap_or_else(|| root.join("index.html"));
    match tokio::fs::read(&target).await {
        Ok(bytes) => {
            let relative = target.strip_prefix(root).unwrap_or(&target);
            let mut response = bytes.into_response();
            let headers = response.headers_mut();
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static(cache_control(relative)),
            );
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
    fn only_hashed_build_output_is_cached_forever() {
        for pinned in [
            "assets/index-B4xQ1k9t.js",
            "assets/index-B4xQ1k9t.css",
            "assets/fonts/inter-B4xQ1k9t.woff2",
            "assets/logo_2-a1b2c3d4.svg",
        ] {
            assert_eq!(cache_control(Path::new(pinned)), IMMUTABLE, "{pinned}");
        }
    }

    #[test]
    fn anything_that_can_change_in_place_must_revalidate() {
        for revalidated in [
            // No content hash: the name survives a deploy, so the body may change.
            "assets/app.js",
            "assets/index-main.js",
            "assets/style.css",
            // The shell points at the current hashes and must never be reused.
            "index.html",
            "favicon.ico",
            // A hash is only trustworthy inside the build-output directory.
            "index-B4xQ1k9t.js",
            "assets-B4xQ1k9t/index.js",
            // Too short or not a base64url token, so it is a name, not a hash.
            "assets/chunk-abc.js",
            "assets/chunk-abcdefghijklmnopqrstuvwxyz0123456.js",
            "assets/chunk-abc.def.gh.js",
        ] {
            assert_eq!(
                cache_control(Path::new(revalidated)),
                REVALIDATE,
                "{revalidated}"
            );
        }
    }
}
