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

/// Build output whose name carries its content hash can never change under that
/// URL, so a client may keep it for a year without asking again.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// `index.html` and every other file keep their URL across a deploy, so a client
/// must revalidate them; otherwise a new shell would never reach a stale tab.
const REVALIDATE: &str = "no-cache";

/// Vite writes every chunk and asset into `assets/` with its content hash in the
/// name, and copies unhashed `public/` files to the root, so the build-output
/// directory is the contract: a file under it cannot change under that URL.
///
/// The name cannot be matched instead of the directory: hashes use the base64url
/// alphabet, so they contain `-` themselves (`amphora-DCKmTK--.js`,
/// `align-horizontal-justify-center-CW-XBPgq.js`), and no suffix rule separates
/// the hash from the name it follows.
fn cache_control(relative: &Path) -> &'static str {
    let is_build_output = relative
        .components()
        .next()
        .is_some_and(|part| part.as_os_str() == "assets");
    if is_build_output {
        IMMUTABLE
    } else {
        REVALIDATE
    }
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

// `tokio::fs::read` uses a blocking worker as well. Keeping this operation in
// one helper lets the concurrency test observe actual handler file reads.
async fn read_file(path: &Path) -> std::io::Result<Vec<u8>> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        #[cfg(test)]
        read_observer::entered(&path);
        std::fs::read(path)
    })
    .await
    .map_err(std::io::Error::other)?
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
        None => {
            let index = root.join("index.html");
            if !index.is_file() {
                return ApiFailure::new(StatusCode::NOT_FOUND, "no_ui", "No UI is installed.")
                    .into_response();
            }
            index
        }
    };
    let (gzip_quality, identity_quality) = encoding_qualities(request.headers());
    if gzip_quality == 0 && identity_quality == 0 {
        let mut response = StatusCode::NOT_ACCEPTABLE.into_response();
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
        return response;
    }
    let try_gzip = gzip_quality > 0
        && (identity_quality == 0 || (gzip_quality >= identity_quality && compressible(&target)));
    #[cfg(test)]
    read_observer::attempted(&target);
    let loaded = if try_gzip {
        #[cfg(test)]
        if COMPRESS_SLOTS.available_permits() == 0 {
            read_observer::waiting(&target);
        }
        let Ok(permit) = COMPRESS_SLOTS.acquire().await else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        let file = target.clone();
        let result = tokio::task::spawn_blocking(move || {
            // A cancelled request cannot release the slot while the read or
            // compression is still running in this blocking worker.
            let _permit = permit;
            #[cfg(test)]
            read_observer::entered(&file);
            let original = std::fs::read(file)?;
            if original.len() < 1024 && identity_quality > 0 {
                return Ok((original, None));
            }
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            let encoded = encoder.write_all(&original).and_then(|()| encoder.finish());
            Ok::<_, std::io::Error>((original, Some(encoded)))
        })
        .await;
        match result {
            Ok(Ok((original, Some(Ok(encoded)))))
                if identity_quality == 0 || encoded.len() < original.len() =>
            {
                Ok((encoded, true))
            }
            Ok(Ok((original, _))) if identity_quality > 0 => Ok((original, false)),
            Ok(Err(error)) => Err(error),
            _ => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    } else {
        read_file(&target).await.map(|bytes| (bytes, false))
    };
    match loaded {
        Ok((bytes, compressed)) => {
            let relative = target.strip_prefix(root).unwrap_or(&target);
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
            if let Ok(value) = HeaderValue::from_str(&etag) {
                headers.insert(header::ETAG, value);
            }
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
mod read_observer {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Condvar, Mutex};
    use tokio::sync::mpsc::UnboundedSender;

    pub(super) struct Hook {
        pub root: PathBuf,
        pub attempts: UnboundedSender<()>,
        pub entries: UnboundedSender<()>,
        pub waiting: UnboundedSender<()>,
        pub gate: (Mutex<bool>, Condvar),
    }

    pub(super) static HOOK: Mutex<Option<Arc<Hook>>> = Mutex::new(None);

    fn matching(path: &Path) -> Option<Arc<Hook>> {
        HOOK.lock()
            .unwrap()
            .as_ref()
            .filter(|hook| path == hook.root)
            .cloned()
    }

    pub(super) fn attempted(path: &Path) {
        if let Some(hook) = matching(path) {
            let _ = hook.attempts.send(());
        }
    }

    pub(super) fn entered(path: &Path) {
        if let Some(hook) = matching(path) {
            let _ = hook.entries.send(());
            let (released, ready) = &hook.gate;
            let mut released = released.lock().unwrap();
            while !*released {
                released = ready.wait(released).unwrap();
            }
        }
    }

    pub(super) fn waiting(path: &Path) {
        if let Some(hook) = matching(path) {
            let _ = hook.waiting.send(());
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn gzip_file_reads_wait_for_admission_even_after_request_cancel() {
        use crate::http::access::Access;
        use orbyn_core::{Core, CoreConfig};
        use std::sync::{Arc, Condvar, Mutex};
        use std::time::Duration;
        use tokio::sync::mpsc;

        let root = std::env::temp_dir().join(format!("orbyn-ui-read-limit-{}", std::process::id()));
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/app.js"), vec![b'a'; 4096]).unwrap();
        std::fs::write(root.join("assets/plain.js"), b"identity").unwrap();
        let mut config = CoreConfig::new(root.join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        let state = AppState {
            core: Arc::new(Core::open(config).unwrap()),
            access: Arc::new(Access::new("token", vec![], None)),
            remote: false,
            ui_dir: Some(root.clone()),
            images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
        };
        let (attempt_tx, mut attempt_rx) = mpsc::unbounded_channel();
        let (entry_tx, mut entry_rx) = mpsc::unbounded_channel();
        let (waiting_tx, mut waiting_rx) = mpsc::unbounded_channel();
        let hook = Arc::new(read_observer::Hook {
            root: root.join("assets/app.js"),
            attempts: attempt_tx,
            entries: entry_tx,
            waiting: waiting_tx,
            gate: (Mutex::new(false), Condvar::new()),
        });
        struct ReleaseOnDrop(Arc<read_observer::Hook>);
        impl Drop for ReleaseOnDrop {
            fn drop(&mut self) {
                let (released, ready) = &self.0.gate;
                *released.lock().unwrap() = true;
                ready.notify_all();
                *read_observer::HOOK.lock().unwrap() = None;
            }
        }
        let _release = ReleaseOnDrop(hook.clone());
        *read_observer::HOOK.lock().unwrap() = Some(hook.clone());
        let request = || {
            Request::builder()
                .uri("/assets/app.js")
                .header("accept-encoding", "gzip")
                .body(axum::body::Body::empty())
                .unwrap()
        };
        let first = tokio::spawn(serve(State(state.clone()), request()));
        let second = tokio::spawn(serve(State(state.clone()), request()));
        for _ in 0..2 {
            tokio::time::timeout(Duration::from_secs(10), entry_rx.recv())
                .await
                .unwrap()
                .unwrap();
        }
        let third = tokio::spawn(serve(State(state.clone()), request()));
        for _ in 0..3 {
            tokio::time::timeout(Duration::from_secs(10), attempt_rx.recv())
                .await
                .unwrap()
                .unwrap();
        }
        tokio::time::timeout(Duration::from_secs(10), waiting_rx.recv())
            .await
            .unwrap()
            .unwrap();
        let identity = Request::builder()
            .uri("/assets/plain.js")
            .body(axum::body::Body::empty())
            .unwrap();
        let identity =
            tokio::time::timeout(Duration::from_secs(10), serve(State(state), identity)).await;
        let entered_while_full =
            tokio::time::timeout(Duration::from_millis(200), entry_rx.recv()).await;
        first.abort();
        let entered_after_cancel =
            tokio::time::timeout(Duration::from_millis(200), entry_rx.recv()).await;
        let (released, ready) = &hook.gate;
        *released.lock().unwrap() = true;
        ready.notify_all();
        // Release the test hook even when the assertions reveal a regression.
        *read_observer::HOOK.lock().unwrap() = None;
        assert!(
            entered_while_full.is_err(),
            "third read began while both slots were busy"
        );
        assert!(
            entered_after_cancel.is_err(),
            "cancelling a request released its in-flight read slot"
        );
        let identity = identity.expect("identity request should not wait for compression");
        assert_eq!(identity.status(), StatusCode::OK);
        assert!(!identity.headers().contains_key(header::CONTENT_ENCODING));
        assert_eq!(second.await.unwrap().status(), StatusCode::OK);
        assert_eq!(third.await.unwrap().status(), StatusCode::OK);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn build_output_is_cached_forever() {
        for pinned in [
            // The 8-character hashes this build writes.
            "assets/index-3hrPZ_u3.js",
            "assets/elk-XV6XMXQE-BHy9YIXy.js",
            // A 7-character hash, which a fixed-length rule would miss.
            "assets/index--td0G00Q.css",
            // Hashes that contain `-` themselves, which no suffix rule can split.
            "assets/amphora-DCKmTK--.js",
            "assets/align-horizontal-justify-center-CW-XBPgq.js",
            "assets/fonts/inter-B4xQ1k9t.woff2",
        ] {
            assert_eq!(cache_control(Path::new(pinned)), IMMUTABLE, "{pinned}");
        }
    }

    #[test]
    fn anything_that_can_change_in_place_must_revalidate() {
        for revalidated in [
            // The shell points at the current hashes and must never be reused.
            "index.html",
            "favicon.ico",
            "manifest.webmanifest",
            // A hash outside the build-output directory proves nothing.
            "index-B4xQ1k9t.js",
            "assets-B4xQ1k9t/index.js",
            "assetsx/app-2.js",
        ] {
            assert_eq!(
                cache_control(Path::new(revalidated)),
                REVALIDATE,
                "{revalidated}"
            );
        }
    }
}
