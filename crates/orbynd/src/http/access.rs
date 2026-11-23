//! Who may reach the service. Every route checks `Host` and `Origin`, which
//! defeats DNS rebinding; `/api`, `/agui` and `/ws` also need the per-launch
//! token or a remote session cookie.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use orbyn_core::conversation::renders::{is_image_name, is_render_id};
use orbyn_core::util::{random_token, secrets_match, sha256_hex};

use crate::http::ApiFailure;

const SESSION_COOKIE: &str = "orbyn_session";
const SESSION_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);

/// Answers whether one render was published with this frame token: the thread id,
/// the render id and the token from the URL. Core owns the folders and the digest,
/// so the middleware asks rather than reads.
pub type FrameCheck = Arc<dyn Fn(&str, &str, &str) -> bool + Send + Sync>;

/// Who made a refused `Person` call, as far as the credential tells. A process that holds the service token (the CLI, or an engine
/// that read `service.json`) is not told apart from another: there is no per-worker token yet
/// (`docs/worker-token-isolation.md`, rollout 4), so a worker shows up here as `service_token`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CallerClass {
    /// The per-launch service token with no browser origin.
    ServiceToken,
    /// A remote session cookie, or a request that carries a browser `Origin`.
    Browser,
}

impl CallerClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ServiceToken => "service_token",
            Self::Browser => "browser",
        }
    }
}

/// One refused `Person` call, for the audit log. Only the method, the matched route pattern (never the URL, so no ids or
/// queries), the caller class and the answer's code: no body, header or token.
#[derive(Clone, Debug)]
pub struct PersonRefusal {
    pub method: String,
    pub route: String,
    /// The name of the one request field a handler refused (`FIELD_LEVEL`), a fixed label and never a value. `None` for a
    /// route that is `Person` as a whole.
    pub field: Option<&'static str>,
    pub caller: CallerClass,
    pub code: &'static str,
}

/// Writes one refusal to the audit log. An error is logged by the caller and never changes the refusal.
pub type RefusalSink = Arc<dyn Fn(&PersonRefusal) -> Result<(), String> + Send + Sync>;

/// A repeat of the same route by the same class inside this window writes no second row.
pub const REFUSAL_WINDOW: Duration = Duration::from_secs(60);
/// Most `(route, caller)` pairs remembered. Routes come from the matched-pattern table, so the real count is far below
/// this; the bound keeps the map small whatever a caller sends.
const REFUSAL_KEYS: usize = 1024;

/// The audit target: `METHOD route`, plus ` [field]` for a handler-level refusal.
pub fn refusal_target(method: &str, route: &str, field: Option<&str>) -> String {
    match field {
        Some(field) => format!("{method} {route} [{field}]"),
        None => format!("{method} {route}"),
    }
}

struct Refusals {
    sink: RefusalSink,
    clock: Arc<dyn Fn() -> Instant + Send + Sync>,
    seen: Mutex<HashMap<(String, CallerClass), Instant>>,
}

pub struct Access {
    token_sha256: String,
    allowed_hosts: Vec<String>,
    remote: Option<Remote>,
    desktop: bool,
    /// `None` until the service hands core's check in, and then no frame token opens
    /// anything.
    frames: Option<FrameCheck>,
    /// SHA-256 of this launch's person key. `None` unless a desktop shell owns the service, and then no
    /// request can present a valid key. The key itself is never stored here.
    person_key_sha256: Option<String>,
    /// Where a refused `Person` call is audited. `None` until the service hands core's writer in.
    refusals: Option<Refusals>,
    /// Development builds only: a `Person` route needs no key (`ORBYN_DEV_DISABLE_PERSON_KEY`).
    #[cfg(debug_assertions)]
    dev_person_open: bool,
}

/// The request header that carries the person key. Only a `Person` route reads it.
pub const PERSON_HEADER: &str = "x-orbyn-person";

/// A browser cannot set a header on a WebSocket, and the key must not go in the URL, so a socket offers it as a
/// subprotocol: `orbyn.person.<key>`, beside [`SOCKET_PROTOCOL`]. The offer is read only on an upgrade request.
pub const PERSON_PROTOCOL_PREFIX: &str = "orbyn.person.";

/// The one subprotocol a terminal socket selects. A browser fails a socket that offered protocols and was answered with
/// none, and the answer must never echo the key.
pub const SOCKET_PROTOCOL: &str = "orbyn.v1";

/// The key a request presents: the header, or on a WebSocket upgrade the `orbyn.person.` subprotocol.
fn person_candidate(headers: &HeaderMap) -> Option<&str> {
    if let Some(value) = headers
        .get(PERSON_HEADER)
        .and_then(|value| value.to_str().ok())
    {
        return Some(value);
    }
    let upgrade = headers
        .get(header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    if !upgrade {
        return None;
    }
    headers
        .get_all(header::SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .find_map(|protocol| protocol.trim().strip_prefix(PERSON_PROTOCOL_PREFIX))
}

struct Remote {
    key_sha256: String,
    sessions: Mutex<HashMap<String, Instant>>,
}

impl Access {
    pub fn new(token: &str, allowed_hosts: Vec<String>, remote_key_sha256: Option<String>) -> Self {
        Self {
            token_sha256: sha256_hex(token.as_bytes()),
            desktop: false,
            allowed_hosts: allowed_hosts
                .into_iter()
                .map(|host| host.to_ascii_lowercase())
                .collect(),
            remote: remote_key_sha256.map(|key_sha256| Remote {
                key_sha256,
                sessions: Mutex::new(HashMap::new()),
            }),
            frames: None,
            person_key_sha256: None,
            refusals: None,
            #[cfg(debug_assertions)]
            dev_person_open: false,
        }
    }

    /// Audits each refused `Person` call through `sink`, once per route and caller class a minute.
    pub fn with_refusal_audit(self, sink: RefusalSink) -> Self {
        self.with_refusal_audit_clock(sink, Arc::new(Instant::now))
    }

    /// As [`Self::with_refusal_audit`] with the clock the window reads, so a test needs no minute of waiting.
    pub fn with_refusal_audit_clock(
        mut self,
        sink: RefusalSink,
        clock: Arc<dyn Fn() -> Instant + Send + Sync>,
    ) -> Self {
        self.refusals = Some(Refusals {
            sink,
            clock,
            seen: Mutex::new(HashMap::new()),
        });
        self
    }

    /// The answer's code, which `person_check` gives for a refusal: a service with no shell has no key.
    fn refusal_code(&self) -> &'static str {
        if self.person_key_sha256.is_some() {
            "person_required"
        } else {
            "desktop_required"
        }
    }

    /// Who is calling, from the credential and the origin. `require_auth` has already passed.
    pub fn caller_class(&self, headers: &HeaderMap) -> CallerClass {
        if self.cookie_valid(headers) || headers.contains_key(header::ORIGIN) {
            CallerClass::Browser
        } else {
            CallerClass::ServiceToken
        }
    }

    /// `person_check` for a handler that checks the key for part of its request (`FIELD_LEVEL`). A refusal is audited
    /// through the same path as the route-level one, under `field`, and returned unchanged. `route` is the matched route
    /// pattern (`MatchedPath`), never the URL.
    pub fn person_check_field(
        &self,
        headers: &HeaderMap,
        method: &str,
        route: &str,
        field: &'static str,
    ) -> Result<(), ApiFailure> {
        self.person_check(headers).inspect_err(|_| {
            self.audit_person_refusal(method, route, Some(field), self.caller_class(headers));
        })
    }

    /// Writes one audit row for a refused `Person` call unless the same route, field and class already wrote one this
    /// minute. A failed write is logged and nothing more: the caller still gets its refusal.
    pub fn audit_person_refusal(
        &self,
        method: &str,
        route: &str,
        field: Option<&'static str>,
        caller: CallerClass,
    ) {
        let Some(refusals) = &self.refusals else {
            return;
        };
        let now = (refusals.clock)();
        {
            let mut seen = refusals
                .seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let key = (refusal_target(method, route, field), caller);
            if seen
                .get(&key)
                .is_some_and(|last| now.saturating_duration_since(*last) < REFUSAL_WINDOW)
            {
                return;
            }
            if seen.len() >= REFUSAL_KEYS {
                seen.retain(|_, last| now.saturating_duration_since(*last) < REFUSAL_WINDOW);
                if seen.len() >= REFUSAL_KEYS {
                    return;
                }
            }
            seen.insert(key, now);
        }
        let refusal = PersonRefusal {
            method: method.to_owned(),
            route: route.to_owned(),
            field,
            caller,
            code: self.refusal_code(),
        };
        if let Err(error) = (refusals.sink)(&refusal) {
            tracing::warn!(%error, "could not audit a refused person route");
        }
    }

    /// Records this launch's person key, as a digest only. The caller hands the key itself to the desktop
    /// shell on the ready line and keeps it nowhere else.
    pub fn with_person_key(mut self, key: Option<&str>) -> Self {
        self.person_key_sha256 = key.map(|key| sha256_hex(key.as_bytes()));
        self
    }

    /// Development builds only, and off unless asked for: lets a `Person` route through with no key, so
    /// web tests and local development work without a desktop shell. Not compiled into a release build.
    #[cfg(debug_assertions)]
    pub fn with_dev_person_override(mut self, open: bool) -> Self {
        self.dev_person_open = open;
        self
    }

    /// Whether this request may use a `Person` route: it carries this launch's person key. A service with
    /// no desktop shell has no key, and answers `desktop_required`.
    pub fn person_check(&self, headers: &HeaderMap) -> Result<(), ApiFailure> {
        #[cfg(debug_assertions)]
        if self.dev_person_open {
            return Ok(());
        }
        let Some(digest) = &self.person_key_sha256 else {
            return Err(ApiFailure::forbidden(
                "desktop_required",
                "This action is only available in the desktop app.",
            ));
        };
        if person_candidate(headers).is_some_and(|candidate| secrets_match(candidate, digest)) {
            Ok(())
        } else {
            Err(ApiFailure::forbidden(
                "person_required",
                "This action needs the person at the desktop app.",
            ))
        }
    }

    /// Only a shell-owned process accepts the two exact native asset origins.
    pub fn with_desktop(mut self, desktop: bool) -> Self {
        self.desktop = desktop;
        self
    }

    /// Teaching this who may open a published page's frame. The service passes a check
    /// over core's render folders; without it a frame token opens nothing.
    pub fn with_frames(mut self, frames: FrameCheck) -> Self {
        self.frames = Some(frames);
        self
    }

    fn host_allowed(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        let name = host_name(&host);
        matches!(name, "localhost" | "127.0.0.1" | "[::1]")
            || self.allowed_hosts.iter().any(|allowed| {
                *allowed == host || (!allowed.contains(':') && allowed.as_str() == name)
            })
    }

    fn token_valid(&self, candidate: &str) -> bool {
        secrets_match(candidate, &self.token_sha256)
    }

    fn cookie_valid(&self, headers: &HeaderMap) -> bool {
        let Some(remote) = &self.remote else {
            return false;
        };
        let Some(value) = cookie_value(headers) else {
            return false;
        };
        let digest = sha256_hex(value.as_bytes());
        let mut sessions = remote
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        sessions.retain(|_, expires| *expires > Instant::now());
        sessions.contains_key(&digest)
    }

    /// Exchanges the remote access key for a session. `None` means refused.
    pub fn open_session(&self, key: &str) -> Option<String> {
        let remote = self.remote.as_ref()?;
        if !secrets_match(key, &remote.key_sha256) {
            return None;
        }
        let session = random_token().ok()?;
        remote
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                sha256_hex(session.as_bytes()),
                Instant::now() + SESSION_LIFETIME,
            );
        Some(session)
    }

    pub fn session_cookie(session: &str, secure: bool) -> String {
        format!(
            "{SESSION_COOKIE}={session}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{}",
            SESSION_LIFETIME.as_secs(),
            if secure { "; Secure" } else { "" }
        )
    }
}

/// The host without its port, keeping IPv6 brackets.
fn host_name(host: &str) -> &str {
    if host.starts_with('[') {
        return host.split_inclusive(']').next().unwrap_or(host);
    }
    host.split(':').next().unwrap_or(host)
}

fn cookie_value(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(&format!("{SESSION_COOKIE}=")))
}

fn origin_matches(origin: &str, host: &str) -> bool {
    origin
        .split_once("://")
        .is_some_and(|(_, authority)| authority.eq_ignore_ascii_case(host))
}

/// Applies to every route, static files and MCP included.
pub async fn guard_origin(
    State(access): State<Arc<Access>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, ApiFailure> {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .or_else(|| {
            request
                .uri()
                .authority()
                .map(|authority| authority.as_str())
        });
    let Some(host) = host.filter(|host| access.host_allowed(host)) else {
        return Err(ApiFailure::forbidden(
            "host_not_allowed",
            "This host is not allowed.",
        ));
    };
    let origin = request.headers().get(header::ORIGIN).cloned();
    let native = access.desktop
        && matches!(host_name(host), "127.0.0.1" | "localhost" | "[::1]")
        && origin
            .as_ref()
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| matches!(value, "tauri://localhost" | "http://tauri.localhost"));
    if let Some(origin) = &origin
        && !native
        && !origin
            .to_str()
            .is_ok_and(|origin| origin_matches(origin, host))
    {
        return Err(ApiFailure::forbidden(
            "origin_not_allowed",
            "This origin is not allowed.",
        ));
    }
    if native && request.method() == Method::OPTIONS {
        if !native_preflight(&request) {
            return Err(ApiFailure::forbidden(
                "origin_not_allowed",
                "This request is not allowed.",
            ));
        }
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        native_headers(response.headers_mut(), origin.expect("native origin"));
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, HEAD, POST, PUT, PATCH, DELETE"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(NATIVE_HEADERS),
        );
        return Ok(response);
    }
    let mut response = next.run(request).await;
    if native {
        native_headers(response.headers_mut(), origin.expect("native origin"));
    }
    Ok(response)
}

/// The headers a desktop page may send across origins. `x-orbyn-confirm` carries the nonce a
/// confirmed account action, or a held command the person approved in the desktop app, sends back; allowing the header grants
/// nothing alone, the nonce is still checked by the account guard or the core's presence grant. `x-orbyn-person` carries the
/// person key a `Person` route checks (`routes/classes.rs`); the key is checked there, not by being allowed here.
const NATIVE_HEADERS: &str = "authorization, content-type, accept, last-event-id, range, if-range, if-none-match, x-orbyn-confirm, x-orbyn-person";

fn native_preflight(request: &Request<Body>) -> bool {
    let path = request.uri().path();
    if !(path.starts_with("/api/") || path.starts_with("/ws/")) {
        return false;
    }
    let method = request
        .headers()
        .get(header::ACCESS_CONTROL_REQUEST_METHOD)
        .and_then(|value| value.to_str().ok());
    if !matches!(
        method,
        Some("GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE")
    ) {
        return false;
    }
    request
        .headers()
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .is_none_or(|value| {
            value.to_str().is_ok_and(|text| {
                text.split(',').all(|name| {
                    NATIVE_HEADERS
                        .split(',')
                        .any(|allowed| allowed.trim().eq_ignore_ascii_case(name.trim()))
                })
            })
        })
}

fn native_headers(headers: &mut HeaderMap, origin: HeaderValue) {
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    headers.append(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("content-range, accept-ranges, etag, content-disposition"),
    );
}

/// Applies to `/api`, `/agui` and `/ws`.
pub async fn require_auth(
    State(access): State<Arc<Access>>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, ApiFailure> {
    if authorized(&access, &request) {
        Ok(next.run(request).await)
    } else {
        Err(ApiFailure::unauthorized())
    }
}

fn authorized(access: &Access, request: &Request<Body>) -> bool {
    let bearer = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if bearer.is_some_and(|token| access.token_valid(token)) {
        return true;
    }
    if access.cookie_valid(request.headers()) {
        return true;
    }
    // Browsers cannot set headers on EventSource, WebSocket, an iframe or an image
    // or video element, so only those read routes take a credential in the URL.
    let path = request.uri().path();
    let render = render_path(path);
    let streaming = matches!(*request.method(), Method::GET | Method::HEAD)
        && (path.starts_with("/ws/")
            || path.ends_with("/events")
            || path.ends_with("/files/raw")
            || path == "/api/media/image"
            || render.is_some()
            || project_image_read(path));
    if !streaming {
        return false;
    }
    if query_token(request).is_some_and(|token| access.token_valid(token)) {
        return true;
    }
    // A published page runs in a frame, its scripts can read its own URL, and that URL
    // must never carry the service token. `?render=` is a capability for exactly one
    // render instead, and it opens nothing but that render's page and its images: any
    // other path, another render or another thread is refused here, and core still has
    // to find a folder whose stored digest matches.
    let Some((thread, render_id)) = render else {
        return false;
    };
    let Some(capability) = query_param(request, "render") else {
        return false;
    };
    access
        .frames
        .as_ref()
        .is_some_and(|frames| frames(thread, render_id, capability))
}

/// The thread and render a published page's read path names, or `None` for anything
/// else. Only the page route and the image route with a name this service writes are
/// reads of a render; publishing one is a `POST` on the folder and never is.
fn render_path(path: &str) -> Option<(&str, &str)> {
    let tail = path.strip_prefix("/api/threads/")?;
    let parts: Vec<&str> = tail.split('/').collect();
    match *parts.as_slice() {
        [thread, "renders", render] if !thread.is_empty() && is_render_id(render) => {
            Some((thread, render))
        }
        [thread, "renders", render, file]
            if !thread.is_empty() && is_render_id(render) && is_image_name(file) =>
        {
            Some((thread, render))
        }
        _ => None,
    }
}

/// Image elements may address only these registered-project image capabilities.
fn project_image_read(path: &str) -> bool {
    let Some(tail) = path.strip_prefix("/api/projects/") else {
        return false;
    };
    let mut parts = tail.split('/');
    matches!((parts.next(), parts.next(), parts.next(), parts.next()),
        (Some(id), Some("icon" | "images"), Some("raw"), None) if !id.is_empty())
}

fn query_token(request: &Request<Body>) -> Option<&str> {
    query_param(request, "token")
}

/// One value from the query string, exactly as written. A token is hex, so it needs
/// no decoding, and an encoded spelling simply fails to match.
fn query_param<'a>(request: &'a Request<Body>, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}=");
    request
        .uri()
        .query()?
        .split('&')
        .find_map(|pair| pair.strip_prefix(prefix.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access() -> Access {
        Access::new(
            "secret",
            vec!["box.tailnet.ts.net".into(), "proxy.test:8443".into()],
            Some(sha256_hex(b"key")),
        )
    }

    #[test]
    fn loopback_and_configured_hosts_pass_and_others_fail() {
        let access = access();
        for host in [
            "127.0.0.1:1234",
            "localhost:1",
            "LOCALHOST",
            "[::1]:9",
            "box.tailnet.ts.net:80",
            "proxy.test:8443",
        ] {
            assert!(access.host_allowed(host), "{host}");
        }
        for host in [
            "evil.test",
            "127.0.0.1.evil.test",
            "localhost.evil.test:1",
            "proxy.test:9999",
        ] {
            assert!(!access.host_allowed(host), "{host}");
        }
    }

    #[test]
    fn image_query_tokens_are_only_for_exact_read_routes() {
        let access = access();
        for path in [
            "/api/projects/p/icon/raw",
            "/api/projects/p/images/raw",
            "/api/media/image",
        ] {
            for method in [Method::GET, Method::HEAD] {
                let request = Request::builder()
                    .method(method)
                    .uri(format!("{path}?token=secret"))
                    .body(Body::empty())
                    .unwrap();
                assert!(authorized(&access, &request), "{path}");
            }
            let request = Request::builder()
                .method(Method::POST)
                .uri(format!("{path}?token=secret"))
                .body(Body::empty())
                .unwrap();
            assert!(!authorized(&access, &request), "{path}");
        }
        for path in [
            "/api/projects/p/icon/resource",
            "/api/settings/projects/image",
            "/api/projects/p/nested/icon/raw",
            "/api/projects//images/raw",
            "/other/projects/p/icon/raw",
            "/api/media/image/extra",
            "/api/media/images",
        ] {
            let request = Request::builder()
                .uri(format!("{path}?token=secret"))
                .body(Body::empty())
                .unwrap();
            assert!(!authorized(&access, &request), "{path}");
        }
    }

    #[test]
    fn render_query_tokens_are_only_for_reads_of_that_render() {
        let access = access();
        let id = "01jc8xw9kq0000000000000000";
        for path in [
            format!("/api/threads/t1/renders/{id}"),
            format!("/api/threads/t1/renders/{id}/image-1.png"),
        ] {
            for method in [Method::GET, Method::HEAD] {
                let request = Request::builder()
                    .method(method)
                    .uri(format!("{path}?token=secret"))
                    .body(Body::empty())
                    .unwrap();
                assert!(authorized(&access, &request), "{path}");
            }
            let published = Request::builder()
                .method(Method::POST)
                .uri(format!("{path}?token=secret"))
                .body(Body::empty())
                .unwrap();
            assert!(!authorized(&access, &published), "{path}");
        }
        for path in [
            "/api/threads/t1/renders".to_owned(),
            "/api/threads/t1/renders/../secret".to_owned(),
            format!("/api/threads/t1/renders/{id}/index.html"),
            format!("/api/threads/t1/renders/{id}/image-1.png/extra"),
            format!("/api/threads//renders/{id}"),
        ] {
            let request = Request::builder()
                .uri(format!("{path}?token=secret"))
                .body(Body::empty())
                .unwrap();
            assert!(!authorized(&access, &request), "{path}");
        }
    }

    #[test]
    fn a_frame_token_opens_its_own_render_and_nothing_else() {
        const ID: &str = "01jc8xw9kq0000000000000000";
        let other = "01jc8xw9kq0000000000000001";
        // Stands in for core: only t1's ID was published with "frame".
        let access = access().with_frames(Arc::new(|thread: &str, render: &str, token: &str| {
            thread == "t1" && render == ID && token == "frame"
        }));
        let call = |method: Method, path: &str, credential: &str| {
            let request = Request::builder()
                .method(method)
                .uri(format!("{path}?{credential}"))
                .body(Body::empty())
                .unwrap();
            authorized(&access, &request)
        };
        for path in [
            format!("/api/threads/t1/renders/{ID}"),
            format!("/api/threads/t1/renders/{ID}/image-1.png"),
        ] {
            for method in [Method::GET, Method::HEAD] {
                assert!(
                    call(method.clone(), &path, "render=frame"),
                    "{path} {method}"
                );
            }
            // The service token still works where it always did.
            assert!(call(Method::GET, &path, "token=secret"), "{path}");
        }
        for (method, path, credential) in [
            // A wrong token, and no token at all.
            (
                Method::GET,
                format!("/api/threads/t1/renders/{ID}"),
                "render=wrong",
            ),
            (
                Method::GET,
                format!("/api/threads/t1/renders/{ID}"),
                "render=",
            ),
            (Method::GET, format!("/api/threads/t1/renders/{ID}"), "x=1"),
            // Another render of the same thread, and another thread's render id.
            (
                Method::GET,
                format!("/api/threads/t1/renders/{other}"),
                "render=frame",
            ),
            (
                Method::GET,
                format!("/api/threads/t2/renders/{ID}"),
                "render=frame",
            ),
            // A write, a publish, and a file name this service does not write.
            (
                Method::POST,
                format!("/api/threads/t1/renders/{ID}"),
                "render=frame",
            ),
            (
                Method::POST,
                "/api/threads/t1/renders".to_owned(),
                "render=frame",
            ),
            (
                Method::GET,
                format!("/api/threads/t1/renders/{ID}/index.html"),
                "render=frame",
            ),
            (
                Method::GET,
                format!("/api/threads/t1/renders/{ID}/frame.sha256"),
                "render=frame",
            ),
            // The same token anywhere else.
            (Method::GET, "/api/settings".to_owned(), "render=frame"),
            (
                Method::GET,
                "/api/threads/t1/files/raw".to_owned(),
                "render=frame",
            ),
            (
                Method::GET,
                "/api/projects/p/icon/raw".to_owned(),
                "render=frame",
            ),
            (Method::GET, "/ws/threads/t1".to_owned(), "render=frame"),
            // The service token is not a frame token.
            (
                Method::GET,
                format!("/api/threads/t1/renders/{ID}"),
                "render=secret",
            ),
        ] {
            assert!(
                !call(method.clone(), &path, credential),
                "{method} {path} {credential}"
            );
        }
    }

    #[test]
    fn a_frame_token_opens_nothing_when_core_is_not_wired_in() {
        const ID: &str = "01jc8xw9kq0000000000000000";
        let access = access();
        let request = Request::builder()
            .uri(format!("/api/threads/t1/renders/{ID}?render=frame"))
            .body(Body::empty())
            .unwrap();
        assert!(!authorized(&access, &request));
        // The matching service token still opens the same route.
        let request = Request::builder()
            .uri(format!("/api/threads/t1/renders/{ID}?token=secret"))
            .body(Body::empty())
            .unwrap();
        assert!(authorized(&access, &request));
    }

    #[test]
    fn origin_must_match_host_exactly() {
        assert!(origin_matches("http://127.0.0.1:5", "127.0.0.1:5"));
        assert!(!origin_matches("http://127.0.0.1:6", "127.0.0.1:5"));
        assert!(!origin_matches("http://evil.test", "127.0.0.1:5"));
        assert!(!origin_matches("null", "127.0.0.1:5"));
    }

    #[test]
    fn session_needs_the_key_and_is_stored_hashed() {
        let access = access();
        assert!(access.open_session("wrong").is_none());
        let session = access.open_session("key").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("a=b; orbyn_session={session}").parse().unwrap(),
        );
        assert!(access.cookie_valid(&headers));
        headers.insert(header::COOKIE, "orbyn_session=forged".parse().unwrap());
        assert!(!access.cookie_valid(&headers));
    }

    fn with_person(value: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(value) = value {
            headers.insert(PERSON_HEADER, value.parse().unwrap());
        }
        headers
    }

    fn code(result: Result<(), ApiFailure>) -> Option<String> {
        use axum::response::IntoResponse;
        let failure = result.err()?;
        let body = futures_util::FutureExt::now_or_never(axum::body::to_bytes(
            failure.into_response().into_body(),
            4096,
        ))?
        .ok()?;
        serde_json::from_slice::<serde_json::Value>(&body).ok()?["code"]
            .as_str()
            .map(str::to_owned)
    }

    #[test]
    fn a_person_route_needs_this_launchs_key() {
        let access = access().with_person_key(Some("the-key"));
        assert!(access.person_check(&with_person(Some("the-key"))).is_ok());
        for wrong in [
            None,
            Some(""),
            Some("the-ke"),
            Some("the-keyy"),
            Some("secret"),
        ] {
            assert_eq!(
                code(access.person_check(&with_person(wrong))).as_deref(),
                Some("person_required"),
                "{wrong:?}"
            );
        }
    }

    #[test]
    fn a_socket_upgrade_may_offer_the_key_as_a_subprotocol_and_nothing_else_may() {
        let access = access().with_person_key(Some("the-key"));
        let socket = |protocols: &str, upgrade: bool| {
            let mut headers = HeaderMap::new();
            headers.insert(header::SEC_WEBSOCKET_PROTOCOL, protocols.parse().unwrap());
            if upgrade {
                headers.insert(header::UPGRADE, "websocket".parse().unwrap());
            }
            headers
        };
        assert!(
            access
                .person_check(&socket("orbyn.v1, orbyn.person.the-key", true))
                .is_ok()
        );
        for refused in [
            socket("orbyn.v1, orbyn.person.the-ke", true),
            socket("orbyn.v1", true),
            // The offer counts on an upgrade only, so a plain request cannot use it.
            socket("orbyn.v1, orbyn.person.the-key", false),
        ] {
            assert_eq!(
                code(access.person_check(&refused)).as_deref(),
                Some("person_required")
            );
        }
    }

    #[test]
    fn without_a_shell_there_is_no_key_to_match() {
        let access = access();
        for sent in [None, Some("the-key"), Some("")] {
            assert_eq!(
                code(access.person_check(&with_person(sent))).as_deref(),
                Some("desktop_required"),
                "{sent:?}"
            );
        }
    }

    #[cfg(debug_assertions)]
    #[test]
    fn the_development_override_is_the_only_way_through_without_a_key() {
        assert_eq!(
            code(access().person_check(&with_person(None))).as_deref(),
            Some("desktop_required")
        );
        let open = access().with_dev_person_override(true);
        assert!(open.person_check(&with_person(None)).is_ok());
        let off = access().with_dev_person_override(false);
        assert_eq!(
            code(off.person_check(&with_person(None))).as_deref(),
            Some("desktop_required")
        );
    }

    #[test]
    fn local_mode_has_no_sessions() {
        let access = Access::new("secret", Vec::new(), None);
        assert!(access.open_session("anything").is_none());
    }

    #[test]
    fn cookie_attributes_match_the_contract() {
        let cookie = Access::session_cookie("v", false);
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Strict"));
        assert!(!cookie.contains("Secure"));
        assert!(Access::session_cookie("v", true).contains("Secure"));
    }
}
