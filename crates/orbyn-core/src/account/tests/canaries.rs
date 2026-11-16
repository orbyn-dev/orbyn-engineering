//! Canaries for the real adapter. The secrets of a signed-in device (its keys, its session token,
//! the vault key, the zone keys and the sealed snapshot's bytes) are read back from the key store
//! and the snapshot, in every form a leak could take, and must be absent from everything the
//! adapter writes out except the two files that exist to hold them: the logs, the error strings,
//! the `Debug` text of the types that hold them, the kept warnings the diagnostics page shows, the
//! telemetry export, and every file in the data folder other than the key store (which holds the
//! keys, not the zone keys or the snapshot) and the sealed snapshot (which holds none in the clear).
//!
//! The values are made at run time, so no secret is written in this file. A Server that echoes a
//! secret in its error code is part of the test: the adapter must not repeat what the Engine says.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata};

use orbyn_device_client::KeyStore as _;

use super::engine_server::Server;
use super::sealed::{Device, FIRST, SECOND, synced};
use super::*;
use crate::account::DeviceApi as _;
use crate::account::api::AccountApi as _;
use crate::account::dev::PASSWORD;
use crate::account::engine::{SnapshotStore as _, Vault as _};
use crate::operations::telemetry_export::{Endpoints, Exporter};

// ---- the capture: a global subscriber at TRACE that keeps every line and every span -------------

static LINES: Mutex<String> = Mutex::new(String::new());
/// The most the capture keeps; older lines go first.
const CAP: usize = 8 * 1024 * 1024;

fn keep(line: &str) {
    let mut lines = LINES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    lines.push_str(line);
    lines.push('\n');
    if lines.len() > CAP {
        let mut cut = lines.len() - CAP;
        while !lines.is_char_boundary(cut) {
            cut += 1;
        }
        lines.drain(..cut);
    }
}

#[derive(Default)]
struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write as _;
        let _ = write!(self.0, " {}={value:?}", field.name());
    }
}

/// Every event and every span field of every thread of this test process, at the most verbose
/// level. Warnings and errors also go to the diagnostics page's keeper, as the service's own
/// layer does, and from there to the telemetry export.
struct Capture;

impl tracing::Subscriber for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mut fields = Fields::default();
        attributes.record(&mut fields);
        keep(&format!(
            "SPAN {}{}",
            attributes.metadata().name(),
            fields.0
        ));
        Id::from_u64(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    fn record(&self, _: &Id, values: &Record<'_>) {
        let mut fields = Fields::default();
        values.record(&mut fields);
        keep(&format!("SPAN-RECORD{}", fields.0));
    }

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let meta = event.metadata();
        keep(&format!("{} {}{}", meta.level(), meta.target(), fields.0));
        if *meta.level() <= tracing::Level::WARN {
            crate::operations::diagnostics::note_event(
                meta.level().as_str(),
                meta.target(),
                fields.0.trim_start().to_owned(),
            );
        }
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

fn capture() {
    static ON: OnceLock<()> = OnceLock::new();
    ON.get_or_init(|| {
        tracing::subscriber::set_global_default(Capture)
            .expect("no other test of this binary sets a global subscriber");
    });
}

fn logged() -> String {
    LINES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

// ---- the canaries: each secret in every form it could be written ---------------------------------

#[derive(Clone)]
struct Canary {
    label: &'static str,
    forms: Vec<Vec<u8>>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A secret made of bytes: raw, in base64 (standard and URL-safe, padded and not), in hex (both cases) and printed
/// as a list of numbers, which is what `{:?}` of a byte slice writes.
fn bytes_of(label: &'static str, bytes: &[u8]) -> Canary {
    assert!(bytes.len() >= 16, "{label}: too short to be a canary");
    let list = format!("{bytes:?}");
    Canary {
        label,
        forms: vec![
            bytes.to_vec(),
            STANDARD.encode(bytes).into_bytes(),
            STANDARD_NO_PAD.encode(bytes).into_bytes(),
            URL_SAFE.encode(bytes).into_bytes(),
            URL_SAFE_NO_PAD.encode(bytes).into_bytes(),
            hex(bytes).into_bytes(),
            hex(bytes).to_uppercase().into_bytes(),
            list.into_bytes(),
        ],
    }
}

/// A secret made of text: as written, in base64 (standard and URL-safe, padded and not) and in hex.
fn text_of(label: &'static str, text: &str) -> Canary {
    assert!(text.len() >= 16, "{label}: too short to be a canary");
    Canary {
        label,
        forms: vec![
            text.as_bytes().to_vec(),
            STANDARD.encode(text).into_bytes(),
            STANDARD_NO_PAD.encode(text).into_bytes(),
            URL_SAFE.encode(text).into_bytes(),
            URL_SAFE_NO_PAD.encode(text).into_bytes(),
            hex(text.as_bytes()).into_bytes(),
            hex(text.as_bytes()).to_uppercase().into_bytes(),
        ],
    }
}

fn found(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// The labels of the canaries that are in `haystack`.
fn leaked<'a>(canaries: &'a [Canary], haystack: &[u8]) -> Vec<&'a str> {
    canaries
        .iter()
        .filter(|c| c.forms.iter().any(|form| found(haystack, form)))
        .map(|c| c.label)
        .collect()
}

/// What a signed-in device holds, read back from its own files.
struct Secrets {
    /// The device keys, the vault key and the session token: what the key store holds on purpose.
    identity: Vec<Canary>,
    /// The zone keys and the snapshot in the clear: what the sealed file holds, sealed.
    zone: Vec<Canary>,
    /// The sealed file's own bytes.
    sealed: Vec<Canary>,
    session_token: String,
}

impl Secrets {
    fn all(&self) -> Vec<Canary> {
        self.identity
            .iter()
            .chain(&self.zone)
            .chain(&self.sealed)
            .cloned()
            .collect()
    }
}

fn secrets(d: &Device, needs_zone_key: bool) -> Secrets {
    let id = d.files.keys.load().unwrap().expect("a signed-in device");
    let token = id.session.as_ref().expect("a session").token.to_string();
    let snapshot = d.files.snaps.load().expect("a sealed snapshot");
    let sealed = std::fs::read(&d.files.snap_file).unwrap();
    let mut zone = vec![bytes_of(
        "the snapshot in the clear",
        &serde_json::to_vec(&snapshot).unwrap(),
    )];
    let mut zone_keys = 0;
    for zone_snapshot in snapshot.zones.values() {
        for key in zone_snapshot.keys.values() {
            zone_keys += 1;
            // Key ids and keys are both base64url text; the key is what is secret.
            zone.push(text_of("a zone key", key));
            if let Ok(raw) = URL_SAFE_NO_PAD.decode(key) {
                zone.push(bytes_of("a zone key (raw)", &raw));
            }
        }
    }
    assert!(
        zone_keys > 0 || !needs_zone_key,
        "the snapshot holds no zone key: the canary is empty"
    );
    Secrets {
        identity: vec![
            bytes_of("the device keys", id.device_keys.as_slice()),
            bytes_of("the vault key", id.vault_key.as_slice()),
            text_of("the session token", &token),
        ],
        zone,
        sealed: vec![
            bytes_of("the sealed snapshot's bytes", &sealed),
            bytes_of("the sealed snapshot's body", &sealed[sealed.len() - 48..]),
        ],
        session_token: token,
    }
}

// ---- the places a secret could go -----------------------------------------------------------------

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// A collector that keeps the raw bytes of every request it gets.
async fn collector() -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1/signal", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut raw = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let read = stream.read(&mut chunk).await.unwrap_or(0);
                raw.extend_from_slice(&chunk[..read]);
                let text = String::from_utf8_lossy(&raw).into_owned();
                if let Some((head, rest)) = text.split_once("\r\n\r\n") {
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|n| n.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if rest.len() >= length {
                        break;
                    }
                }
                if read == 0 {
                    break;
                }
            }
            kept.lock().unwrap().extend_from_slice(&raw);
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
        }
    });
    (url, seen)
}

fn leaked_ref<'a>(canaries: &[&'a Canary], haystack: &[u8]) -> Vec<&'a str> {
    canaries
        .iter()
        .filter(|c| c.forms.iter().any(|form| found(haystack, form)))
        .map(|c| c.label)
        .collect()
}

/// What the adapter answers with when it fails, in every shape a caller can print.
fn shown<E: std::fmt::Debug + std::fmt::Display>(error: &E) -> String {
    format!("{error:?} | {error} | {error:#?}")
}

#[tokio::test]
async fn no_secret_of_the_real_adapter_reaches_a_log_an_error_a_debug_line_a_warning_an_export_or_a_stray_file()
 {
    capture();
    let dir = TempDir::new();
    let server = Server::new();
    let d = synced(&server, dir.path(), FIRST, "one").await;
    let other = synced(&server, dir.path(), SECOND, "two").await;
    // A first upload makes the zone's key, so this device's snapshot has one to plant. The other
    // device is a second identity in the same folder: its keys are canaries too.
    let record: crate::account::merge::Record = [(
        "preview_appearance".to_owned(),
        crate::account::merge::Field {
            value: serde_json::json!("dark"),
            stamp: crate::account::merge::Stamp {
                wall_ms: crate::util::unix_now_ms(),
                counter: 1,
                device: d.zones.device_tag().unwrap_or_default(),
            },
        },
    )]
    .into();
    d.zones.push(ZONE, None, &record).await.unwrap();
    d.zones.pull(ZONE).await.unwrap();
    let mine = secrets(&d, true);
    let theirs = secrets(&other, false);
    let token = mine.session_token.clone();
    // What a file may hold: the key store has no snapshot secret, the sealed file no secret of the
    // identity, the zone keys or the snapshot in the clear.
    let (my_zone, their_zone) = (
        mine.zone.iter().chain(&mine.sealed).collect::<Vec<_>>(),
        theirs.zone.iter().chain(&theirs.sealed).collect::<Vec<_>>(),
    );
    let in_sealed: Vec<&Canary> = mine
        .identity
        .iter()
        .chain(&mine.zone)
        .chain(theirs.identity.iter())
        .chain(&theirs.zone)
        .collect();
    let in_keys: Vec<&Canary> = my_zone.into_iter().chain(their_zone).collect();
    let key_form = mine.identity[0].forms[1].clone();
    let invite = format!("invite-{}", crate::util::new_id());
    let code = format!("code-{}", crate::util::new_id());
    let mut both: Vec<Canary> = mine.all().into_iter().chain(theirs.all()).collect();
    both.push(text_of("an invite", &invite));
    both.push(text_of("a code", &code));
    let mut printed = String::new();

    // The adapter's own calls, working: a pull, the device list, a password check, the approval
    // requests. Whatever they log or return is part of the output.
    for _ in 0..2 {
        let _ = d.zones.pull(ZONE).await;
    }
    printed += &format!("{:?}\n", d.account.devices().await);
    printed += &format!("{:?}\n", d.account.approval_requests().await);
    let password = crate::account::Password::new(PASSWORD.to_owned());
    printed += &format!("{:?}\n", d.account.verify_password(&password).await);
    // The sign-up and second-factor steps of the same adapter, each with a value to find. Their
    // answers do not matter here; nothing they write may carry the invite or the code.
    printed += &format!("{:?}\n", d.account.check_invite(&invite).await);
    printed += &format!(
        "{:?}\n",
        d.account
            .start_sign_up(&invite, "a@example.com", &password)
            .await
    );
    printed += &format!("{:?}\n", d.account.verify_email_code(&code).await);
    printed += &format!("{:?}\n", d.account.second_factor(&code).await);

    // A hostile or confused Engine: every refusal echoes a secret in its error code. The adapter
    // must map it to its own words and repeat none of it.
    for (part, status) in [
        ("/v1/zones", 401),
        ("/v1/zones", 403),
        ("/v1/zones", 409),
        ("/v1/zones", 500),
        ("/v1/devices", 429),
        ("/v1/devices", 500),
        ("/v1/", 502),
    ] {
        let code = format!("{}-{}", token, status);
        server.refuse(part, status, &code);
        // `Unauthorized` wipes the keys when the worker reacts, but these calls only fail.
        match d.zones.pull(ZONE).await {
            Err(error) => printed += &format!("{}\n", shown(&error)),
            Ok(ok) => printed += &format!("{ok:?}\n"),
        }
        match d.account.devices().await {
            Err(error) => {
                printed += &format!("{}\n", shown(&error));
                printed += &format!("{}\n", crate::account::api::refusal(&error));
            }
            Ok(list) => printed += &format!("{list:?}\n"),
        }
    }
    server.clear_refusal();
    server.set_offline(true);
    printed += &format!("{:?}\n", d.zones.pull(ZONE).await);
    printed += &format!("{:?}\n", d.account.devices().await);
    server.set_offline(false);

    // A sealed snapshot that was changed on disk is refused with a warning, which the keeper and
    // the export both carry.
    let original = std::fs::read(&d.files.snap_file).unwrap();
    let mut changed = original.clone();
    let last = changed.len() - 1;
    changed[last] ^= 0xff;
    std::fs::write(&d.files.snap_file, &changed).unwrap();
    assert!(d.files.snaps.load().is_none(), "the changed file was used");
    assert!(
        logged().contains("the sealed account snapshot was not used"),
        "the refusal's warning was not captured"
    );
    std::fs::write(&d.files.snap_file, &original).unwrap();
    assert!(d.files.snaps.load().is_some());

    // The types that hold the secrets print without them.
    let id = d.files.keys.load().unwrap().unwrap();
    let snapshot = d.files.snaps.load().unwrap();
    printed += &format!(
        "{:?} {:?} {:#?} {:?} {:?} {:?} {:?}\n",
        d.files.keys,
        d.files.snaps,
        id,
        snapshot,
        snapshot.zones.values().collect::<Vec<_>>(),
        d.files.keys.anchor(),
        *d.zones.status().borrow(),
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // 1. Logs and span fields, error strings and Debug text.
    let logs = logged();
    assert!(
        !logs.is_empty(),
        "the capture saw nothing: it is not attached"
    );
    assert_eq!(
        leaked(&both, logs.as_bytes()),
        Vec::<&str>::new(),
        "in a log line"
    );
    assert_eq!(
        leaked(&both, printed.as_bytes()),
        Vec::<&str>::new(),
        "in an error string or a Debug line"
    );

    // 2. The kept warnings and the telemetry export of them.
    let events = crate::operations::diagnostics::recent_events();
    assert!(
        !events.is_empty(),
        "no warning was kept: the refusal's warning is missing"
    );
    let kept: String = events
        .iter()
        .map(|e| format!("{} {} {} {}\n", e.at, e.level, e.target, e.message))
        .collect();
    assert_eq!(
        leaked(&both, kept.as_bytes()),
        Vec::<&str>::new(),
        "in a kept warning"
    );
    let (url, received) = collector().await;
    let exporter = Exporter::new(Endpoints {
        logs: Some(url),
        metrics: None,
    })
    .unwrap();
    for e in &events {
        exporter.offer(&e.at, &e.level, &e.target, &e.message);
    }
    exporter.flush().await;
    let sent = received.lock().unwrap().clone();
    assert!(
        found(&sent, b"resourceLogs"),
        "the export sent no batch: it is not exercised"
    );
    assert_eq!(
        leaked(&both, &sent),
        Vec::<&str>::new(),
        "in the telemetry export"
    );

    // 3. The data folder: every file, by what it is for.
    let mut paths = Vec::new();
    files_under(dir.path(), &mut paths);
    let key_files = [d.files.key_file.clone(), other.files.key_file.clone()];
    let sealed = [d.files.snap_file.clone(), other.files.snap_file.clone()];
    assert!(
        paths.len() >= 4,
        "the data folder is not what the test expects: {paths:?}"
    );
    for path in &paths {
        let bytes = std::fs::read(path).unwrap();
        let name = path.display();
        if key_files.contains(path) {
            assert_eq!(
                leaked_ref(&in_keys, &bytes),
                Vec::<&str>::new(),
                "{name} holds the snapshot"
            );
        } else if sealed.contains(path) {
            assert_eq!(
                leaked_ref(&in_sealed, &bytes),
                Vec::<&str>::new(),
                "{name} holds a secret in the clear"
            );
        } else {
            panic!("{name}: a file that is neither the key store nor the sealed snapshot");
        }
    }
    // The identity is in the key store in the one form it is written in, so the scan can see it.
    let key_bytes = std::fs::read(&d.files.key_file).unwrap();
    assert!(
        found(&key_bytes, &key_form),
        "the scan would not see the device keys in the key store"
    );

    // 4. After a sign-out nothing is left at all.
    d.account.sign_out().await.unwrap();
    let mut left = Vec::new();
    files_under(dir.path(), &mut left);
    for path in left {
        let bytes = std::fs::read(&path).unwrap();
        if path == other.files.key_file || path == other.files.snap_file {
            continue;
        }
        assert_eq!(
            leaked(&both, &bytes),
            Vec::<&str>::new(),
            "{} after sign-out",
            path.display()
        );
    }
    assert_eq!(
        leaked(&both, logged().as_bytes()),
        Vec::<&str>::new(),
        "in a log line, after sign-out"
    );
}
