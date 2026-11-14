//! Approving one held Crew command: the person's click in this app's own native dialog.
//!
//! `crew_presence` is the only way a grant is made. The page asks with a hold id and nothing else. The service describes the hold
//! (the command, the worker, the task, the hold's binding hash), the shell shows that in a native dialog, and only when the person
//! clicks Approve once does the shell mint a one-time nonce, send it to the service bound to the hold as the service described it,
//! and wait for the service to record it. The nonce reaches the page only as this command's return value, after `presence_ready`.
//!
//! Everything shown comes from the service, never from the page. One flow runs at a time: a second call while one is open answers
//! `busy`, so dialogs never stack and a hold is never asked about twice at once.
//!
//! This is not OS-level presence (docs/crew.md, "Approving once: local presence"): a process of the same OS user can still drive
//! the native dialog, read this process's memory or write into the service's pipe. The nonce is never logged, never written to
//! disk and never part of an error; the buffers this file owns are zeroed when dropped, which is best effort (the copy that goes
//! to the page and the line to the service are not ours to wipe).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::{Serialize, Serializer};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};

/// How long the service has to answer each of the two requests.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

const ID_LIMIT: usize = 128;
const COMMAND_LIMIT: usize = 2000;
const NAME_LIMIT: usize = 200;
const TITLE_LIMIT: usize = 400;
const CODE_LIMIT: usize = 32;

pub const DIALOG_TITLE: &str = "Approve this command once?";
pub const APPROVE_LABEL: &str = "Approve once";
pub const CANCEL_LABEL: &str = "Cancel";
/// How long the person has to answer an open dialog. A hold waits 10 minutes for the person, so a later answer would be refused anyway.
pub const PERSON_LIMIT: Duration = Duration::from_secs(10 * 60);

const fn same_text(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    while at < a.len() {
        if a[at] != b[at] {
            return false;
        }
        at += 1;
    }
    true
}
// A dismissal reads as the Cancel label, so the two labels must never be the same.
const _: () = assert!(!same_text(APPROVE_LABEL, CANCEL_LABEL));

const DIALOG_FOOTER: &str = "This approves this one command. It cannot be reused.";

/// Why `crew_presence` gave no nonce. The page gets [`PresenceError::code`]; no variant holds a nonce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresenceError {
    /// The hold id is empty, too long or has characters an id never has.
    InvalidHold,
    /// The caller is not the bundled app's main window.
    Forbidden,
    /// Another approval is open.
    Busy,
    /// No service is running to ask.
    Unavailable,
    /// The service did not answer in time.
    Timeout,
    /// The person did not click Approve once.
    Cancelled,
    /// The service refused, with its code (`presence_refused:<code>`).
    Refused(String),
    /// The service's answer was not what was asked for, or held text a dialog must not show.
    InvalidAnswer,
    /// The dialog could not be shown.
    Dialog,
    /// The operating system gave no random bytes.
    Nonce,
}

impl PresenceError {
    /// The stable string the page matches on.
    pub fn code(&self) -> String {
        match self {
            Self::InvalidHold => "invalid_hold".into(),
            Self::Forbidden => "forbidden".into(),
            Self::Busy => "busy".into(),
            Self::Unavailable => "unavailable".into(),
            Self::Timeout => "timeout".into(),
            Self::Cancelled => "cancelled".into(),
            Self::Refused(code) => format!("presence_refused:{code}"),
            Self::InvalidAnswer => "invalid_answer".into(),
            Self::Dialog => "dialog_failed".into(),
            Self::Nonce => "nonce_failed".into(),
        }
    }

    /// A refusal from the service, with a code that is only lowercase letters and underscores.
    fn refused(code: &str) -> Self {
        let clean = !code.is_empty()
            && code.len() <= CODE_LIMIT
            && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
        Self::Refused(if clean { code } else { "unknown" }.into())
    }
}

impl std::fmt::Display for PresenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.code())
    }
}

impl std::error::Error for PresenceError {}

impl Serialize for PresenceError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.code())
    }
}

/// A hold id the service could have made: short, and letters, digits, `-` and `_` only.
pub fn valid_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= ID_LIMIT
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A character that draws nothing or changes how the text around it draws: controls, soft hyphen, zero-width and bidi marks, line and
/// paragraph separators, invisible operators, Hangul fillers, variation selectors, the byte order mark, tag characters. The same set the
/// service turns into a visible mark (copied, not shared), so one arriving here means the text was not made for a plain dialog.
fn invisible(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{115F}' | '\u{1160}' | '\u{17B4}' | '\u{17B5}'
            | '\u{180B}'..='\u{180F}' | '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}' | '\u{3164}' | '\u{FE00}'..='\u{FE0F}' | '\u{FEFF}' | '\u{FFA0}'
            | '\u{FFF9}'..='\u{FFFC}' | '\u{1BCA0}'..='\u{1BCA3}' | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
            // Format characters the service's set does not list yet.
            | '\u{0600}'..='\u{0605}' | '\u{06DD}' | '\u{070F}' | '\u{08E2}' | '\u{13430}'..='\u{1343F}')
}

/// What the service says a hold is. The dialog shows these and the grant is bound to the ids and the hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hold {
    pub hold_id: String,
    pub thread_id: String,
    pub task_id: String,
    pub command_shown: String,
    pub command_sha256: String,
    pub worker_name: String,
    pub task_title: String,
    pub truncated: bool,
}

impl Hold {
    /// Whether this is a hold the shell may show: the ids and the hash have the shape of the service's, the text has no control,
    /// bidirectional, zero-width or format character (the service turns them into visible marks, so one here would be a line or an order it did not mean) and nothing is too long
    /// for a dialog.
    pub fn is_showable(&self) -> bool {
        let fits = |text: &str, limit: usize| text.chars().count() <= limit;
        let plain = |text: &str| !text.chars().any(invisible);
        valid_id(&self.hold_id)
            && valid_id(&self.thread_id)
            && valid_id(&self.task_id)
            && !self.command_sha256.is_empty()
            && self.command_sha256.len() <= ID_LIMIT
            && self.command_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            && !self.command_shown.is_empty()
            && fits(&self.command_shown, COMMAND_LIMIT)
            && fits(&self.worker_name, NAME_LIMIT)
            && fits(&self.task_title, TITLE_LIMIT)
            && plain(&self.command_shown)
            && plain(&self.worker_name)
            && plain(&self.task_title)
    }
}

/// The body of the dialog, as plain text: the command exactly as the service gave it (with an ellipsis if it was cut), the worker,
/// the task and what the click does. Nothing here is escaped or interpreted; it never mentions a time limit.
pub fn dialog_body(hold: &Hold) -> String {
    let cut = if hold.truncated { "\u{2026}" } else { "" };
    format!(
        "{}{}\n\nWorker: {}\nTask: {}\n\n{}",
        hold.command_shown, cut, hold.worker_name, hold.task_title, DIALOG_FOOTER
    )
}

/// `text` made safe for a dialog that reads its text as a printf format. rfd's GTK 3 backend (every platform but macOS and Windows)
/// hands the description to `gtk_message_dialog_format_secondary_text` with no arguments, so a lone `%` in a command would read memory
/// or change what the person sees. Doubling every `%` makes it a literal one.
pub fn double_percent(text: &str) -> String {
    text.replace('%', "%%")
}

/// The body as the platform's dialog needs it: unchanged on macOS and Windows, where it is plain text and a doubled `%` would show,
/// percent-escaped elsewhere.
pub fn platform_body(body: String) -> String {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        body
    } else {
        double_percent(&body)
    }
}

/// The line that asks the service what a hold is.
pub fn describe_line(request_id: &str, hold_id: &str) -> Value {
    json!({ "type": "presence_describe", "request_id": request_id, "hold_id": hold_id })
}

/// The line that records the person's approval, bound to exactly what the service described.
pub fn grant_line(request_id: &str, nonce: &Nonce, hold: &Hold) -> Value {
    json!({
        "type": "presence_grant",
        "request_id": request_id,
        "nonce": nonce.as_str(),
        "hold_id": hold.hold_id,
        "thread_id": hold.thread_id,
        "task_id": hold.task_id,
        "command_sha256": hold.command_sha256,
    })
}

/// The one-time proof, 16 random bytes as 32 lowercase hex characters. It cannot be printed or copied by accident: `Debug` and
/// `Display` show nothing of it, and its bytes are zeroed when it is dropped.
pub struct Nonce([u8; 32]);

impl Nonce {
    /// A fresh nonce from the operating system's random source.
    pub fn mint() -> Result<Self, PresenceError> {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| PresenceError::Nonce)?;
        let nonce = Self::from_bytes(&random);
        wipe(&mut random);
        Ok(nonce)
    }

    fn from_bytes(bytes: &[u8; 16]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = [0u8; 32];
        for (at, byte) in bytes.iter().enumerate() {
            text[at * 2] = HEX[usize::from(byte >> 4)];
            text[at * 2 + 1] = HEX[usize::from(byte & 0x0f)];
        }
        Self(text)
    }

    pub fn as_str(&self) -> &str {
        // Only ever hex digits.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    /// Hands the nonce to the page: a copy for the return value, then this one is wiped.
    fn into_page(self) -> String {
        self.as_str().to_owned()
    }
}

impl std::fmt::Debug for Nonce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Nonce(..)")
    }
}

impl Drop for Nonce {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// Zeroes a buffer in a way the compiler may not drop.
fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: `byte` is a valid, aligned, exclusive reference.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(Ordering::SeqCst);
}

/// What the service answered to one request.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    Hold(Hold),
    Ready,
    Refused(String),
}

/// The requests waiting for an answer, by request id.
#[derive(Default)]
pub struct Requests {
    waiting: Mutex<HashMap<String, Sender<Answer>>>,
    next: AtomicU64,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A request that has been registered and not yet answered. Dropping it forgets the request.
pub struct Pending<'a> {
    id: String,
    receiver: Receiver<Answer>,
    requests: &'a Requests,
}

impl Requests {
    /// Registers a new request. Its id is made only of letters and digits, so the service takes it.
    pub fn open(&self) -> Pending<'_> {
        let id = format!("presence-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let (sender, receiver) = channel();
        locked(&self.waiting).insert(id.clone(), sender);
        Pending {
            id,
            receiver,
            requests: self,
        }
    }

    /// Gives an answer to the request it names. An id nobody waits for (late, repeated or made up) is ignored.
    pub fn deliver(&self, request_id: &str, answer: Answer) -> bool {
        let sender = locked(&self.waiting).remove(request_id);
        sender.is_some_and(|sender| sender.send(answer).is_ok())
    }

    #[cfg(test)]
    fn waiting(&self) -> usize {
        locked(&self.waiting).len()
    }
}

impl Pending<'_> {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Waits for the answer, or gives up after `timeout`.
    pub fn wait(self, timeout: Duration) -> Result<Answer, PresenceError> {
        match self.receiver.recv_timeout(timeout) {
            Ok(answer) => Ok(answer),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                Err(PresenceError::Timeout)
            }
        }
    }
}

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        locked(&self.requests.waiting).remove(&self.id);
    }
}

/// The shell's state for this feature: the waiting requests and the one open approval.
#[derive(Default)]
pub struct Presence {
    requests: Requests,
    open: Mutex<bool>,
    /// True while a dialog is on screen, even after its answer stopped being waited for.
    dialog: Arc<AtomicBool>,
}

/// Held while an approval is open. Dropping it lets the next one start.
pub struct Flight<'a>(&'a Presence);

impl Presence {
    /// Starts the one approval that may be open, or says `busy`.
    pub fn begin(&self) -> Result<Flight<'_>, PresenceError> {
        let mut open = locked(&self.open);
        if *open || self.dialog.load(Ordering::SeqCst) {
            return Err(PresenceError::Busy);
        }
        *open = true;
        Ok(Flight(self))
    }

    pub fn requests(&self) -> &Requests {
        &self.requests
    }

    fn dialog_flag(&self) -> Arc<AtomicBool> {
        self.dialog.clone()
    }
}

/// Runs a blocking dialog off this thread and waits for the person at most `limit`. A native dialog cannot be closed from here, so when
/// the limit passes the approval ends with `timeout` and a click that comes later is thrown away, but `flag` stays set until the dialog
/// is really gone: a new approval answers `busy` rather than stacking a second dialog on the first.
pub fn ask_with_limit(
    flag: &Arc<AtomicBool>,
    limit: Duration,
    show: impl FnOnce() -> bool + Send + 'static,
) -> Result<bool, PresenceError> {
    if flag.swap(true, Ordering::SeqCst) {
        return Err(PresenceError::Busy);
    }
    struct Release(Arc<AtomicBool>);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let (sender, receiver) = channel();
    let release = Release(flag.clone());
    std::thread::spawn(move || {
        let _release = release;
        let _ = sender.send(show());
    });
    match receiver.recv_timeout(limit) {
        Ok(approved) => Ok(approved),
        Err(RecvTimeoutError::Timeout) => Err(PresenceError::Timeout),
        Err(RecvTimeoutError::Disconnected) => Err(PresenceError::Dialog),
    }
}

impl Drop for Flight<'_> {
    fn drop(&mut self) {
        *locked(&self.0.open) = false;
    }
}

/// What the flow needs from the app: a way to the service and the person's answer. The real one is [`AppShell`].
pub trait Shell {
    /// Whether there is a service to send to.
    fn service_running(&self) -> bool;
    /// Sends one line to the service.
    fn send(&self, line: &Value);
    /// Shows the dialog and waits for the person. True only for a click on Approve once.
    fn confirm(&self, hold: &Hold) -> Result<bool, PresenceError>;
}

/// Asks about a hold and, if the person approves, returns the nonce the service has recorded. Blocks, so it runs off the UI thread.
pub fn approve(
    presence: &Presence,
    shell: &impl Shell,
    hold_id: &str,
    timeout: Duration,
) -> Result<String, PresenceError> {
    if !valid_id(hold_id) {
        return Err(PresenceError::InvalidHold);
    }
    let _flight = presence.begin()?;
    if !shell.service_running() {
        return Err(PresenceError::Unavailable);
    }

    let described = presence.requests().open();
    shell.send(&describe_line(described.id(), hold_id));
    let hold = match described.wait(timeout)? {
        Answer::Hold(hold) => hold,
        Answer::Refused(code) => return Err(PresenceError::refused(&code)),
        Answer::Ready => return Err(PresenceError::InvalidAnswer),
    };
    // The answer is for the hold that was asked about, and a shape the dialog can show.
    if hold.hold_id != hold_id || !hold.is_showable() {
        return Err(PresenceError::InvalidAnswer);
    }

    if !shell.confirm(&hold)? {
        return Err(PresenceError::Cancelled);
    }

    let nonce = Nonce::mint()?;
    let granted = presence.requests().open();
    shell.send(&grant_line(granted.id(), &nonce, &hold));
    match granted.wait(timeout)? {
        Answer::Ready => Ok(nonce.into_page()),
        Answer::Refused(code) => Err(PresenceError::refused(&code)),
        Answer::Hold(_) => Err(PresenceError::InvalidAnswer),
    }
}

/// The app's own shell: the service pipe and the native dialog.
struct AppShell {
    app: AppHandle,
    window: WebviewWindow,
}

impl Shell for AppShell {
    fn service_running(&self) -> bool {
        self.app
            .state::<crate::Core>()
            .0
            .lock()
            .is_ok_and(|core| core.is_some())
    }

    fn send(&self, line: &Value) {
        crate::tell_core(&self.app, line.clone());
    }

    fn confirm(&self, hold: &Hold) -> Result<bool, PresenceError> {
        // Bring the main window forward first, and never open a dialog while it is hidden. The dialog is parented to it on macOS and
        // Windows; rfd's GTK 3 backend ignores a parent, so on Linux it is unparented and may open behind the window.
        let window = self.window.clone();
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        // Un-minimising animates on macOS, so give it a moment before giving up.
        let mut shown = false;
        for _ in 0..10 {
            if window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true) {
                shown = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if !shown {
            return Err(PresenceError::Unavailable);
        }
        let app = self.app.clone();
        let body = platform_body(dialog_body(hold));
        let flag = self.app.state::<Presence>().dialog_flag();
        // Approve once is the OK slot and Cancel the cancel slot, so what the plugin returns for Escape, the close box or a failed
        // call is the Cancel label (see `dismissals_never_read_as_approval`). On macOS and Windows the first button, Approve once, is
        // the default one: Return approves, so this dialog may only be opened for the person's own click.
        ask_with_limit(&flag, PERSON_LIMIT, move || {
            let result = app
                .dialog()
                .message(body)
                .title(DIALOG_TITLE)
                .parent(&window)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    APPROVE_LABEL.into(),
                    CANCEL_LABEL.into(),
                ))
                .blocking_show_with_result();
            approved(&result)
        })
    }
}

/// Whether a dialog result is a click on Approve once.
pub fn approved(result: &MessageDialogResult) -> bool {
    matches!(result, MessageDialogResult::Custom(label) if label == APPROVE_LABEL)
}

// The dialog is opened from exactly one place: `AppShell::confirm`, which `approve` calls once per `crew_presence`. Nothing retries,
// reopens after a refusal or queues a second one (`approve_never_reopens_the_dialog`).

/// Hands the service's answer to the request that waits for it.
pub fn deliver(app: &AppHandle, request_id: &str, answer: Answer) {
    app.state::<Presence>()
        .requests()
        .deliver(request_id, answer);
}

/// Only the bundled app's main window may ask.
fn require_caller(window: &WebviewWindow) -> Result<(), PresenceError> {
    let url = window.url().map_err(|_| PresenceError::Forbidden)?;
    if window.label() == "main" && crate::desktop_origin::is_application(&url) {
        Ok(())
    } else {
        Err(PresenceError::Forbidden)
    }
}

/// Asks the person to approve one held command and returns the nonce the page sends with its approval.
#[tauri::command]
pub async fn crew_presence(
    app: AppHandle,
    window: WebviewWindow,
    hold_id: String,
) -> Result<String, PresenceError> {
    require_caller(&window)?;
    if !valid_id(&hold_id) {
        return Err(PresenceError::InvalidHold);
    }
    // The dialog blocks until the person answers, so it never runs on the async runtime's own threads.
    tauri::async_runtime::spawn_blocking(move || {
        approve(
            &app.state::<Presence>(),
            &AppShell {
                app: app.clone(),
                window,
            },
            &hold_id,
            ANSWER_TIMEOUT,
        )
    })
    .await
    .map_err(|_| PresenceError::Dialog)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn hold() -> Hold {
        Hold {
            hold_id: "h-1".into(),
            thread_id: "w-1".into(),
            task_id: "t-1".into(),
            command_shown: "make deploy --to prod".into(),
            command_sha256: "ab".repeat(32),
            worker_name: "Maya".into(),
            task_title: "Ship the release".into(),
            truncated: false,
        }
    }

    fn is_nonce(text: &str) -> bool {
        text.len() == 32 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }

    /// A shell that answers as the service would, and records what it was sent.
    struct Fake {
        running: bool,
        describe: Mutex<Option<Answer>>,
        grant: Mutex<Option<Answer>>,
        click: Mutex<Result<bool, PresenceError>>,
        sent: Mutex<Vec<Value>>,
        shown: Mutex<Vec<Hold>>,
        presence: Arc<Presence>,
    }

    impl Fake {
        fn new(presence: &Arc<Presence>) -> Self {
            Self {
                running: true,
                describe: Mutex::new(Some(Answer::Hold(hold()))),
                grant: Mutex::new(Some(Answer::Ready)),
                click: Mutex::new(Ok(true)),
                sent: Mutex::new(Vec::new()),
                shown: Mutex::new(Vec::new()),
                presence: presence.clone(),
            }
        }
    }

    impl Shell for Fake {
        fn service_running(&self) -> bool {
            self.running
        }

        fn send(&self, line: &Value) {
            locked(&self.sent).push(line.clone());
            let id = line["request_id"].as_str().unwrap().to_owned();
            let answer = match line["type"].as_str().unwrap() {
                "presence_describe" => locked(&self.describe).take(),
                _ => locked(&self.grant).take(),
            };
            if let Some(answer) = answer {
                self.presence.requests().deliver(&id, answer);
            }
        }

        fn confirm(&self, hold: &Hold) -> Result<bool, PresenceError> {
            locked(&self.shown).push(hold.clone());
            locked(&self.click).clone()
        }
    }

    const SHORT: Duration = Duration::from_millis(60);

    #[test]
    fn approve_never_reopens_the_dialog() {
        let presence = Arc::new(Presence::default());
        // Cancelled: one dialog, no second one, nothing sent after.
        let shell = Fake::new(&presence);
        *locked(&shell.click) = Ok(false);
        assert!(approve(&presence, &shell, "h-1", SHORT).is_err());
        assert_eq!(locked(&shell.shown).len(), 1);
        assert_eq!(locked(&shell.sent).len(), 1);
        // The service refuses the grant: still one dialog.
        let shell = Fake::new(&presence);
        *locked(&shell.grant) = Some(Answer::Refused("gone".into()));
        assert!(approve(&presence, &shell, "h-1", SHORT).is_err());
        assert_eq!(locked(&shell.shown).len(), 1);
        // The grant times out: still one dialog.
        let shell = Fake::new(&presence);
        *locked(&shell.grant) = None;
        assert_eq!(
            approve(&presence, &shell, "h-1", SHORT),
            Err(PresenceError::Timeout)
        );
        assert_eq!(locked(&shell.shown).len(), 1);
        // Refused or silent before the dialog: none at all.
        for describe in [Some(Answer::Refused("gone".into())), None] {
            let shell = Fake::new(&presence);
            *locked(&shell.describe) = describe;
            assert!(approve(&presence, &shell, "h-1", SHORT).is_err());
            assert!(locked(&shell.shown).is_empty());
        }
        // A dialog that failed to show is not shown again.
        let shell = Fake::new(&presence);
        *locked(&shell.click) = Err(PresenceError::Unavailable);
        assert!(approve(&presence, &shell, "h-1", SHORT).is_err());
        assert_eq!(locked(&shell.shown).len(), 1);
    }

    /// What `rfd` 0.17.2 hands the plugin and what `tauri-plugin-dialog` 2.8.4 (`desktop.rs`, `show_message_dialog`) makes of it for
    /// `OkCancelCustom(ok, cancel)`: its `Ok` becomes `Custom(ok)`, its `Cancel` becomes `Custom(cancel)`, anything else passes on.
    #[derive(Clone, Debug)]
    enum Rfd {
        Ok,
        Cancel,
        Custom(String),
    }

    fn plugin(result: Rfd, ok: &str, cancel: &str) -> MessageDialogResult {
        match result {
            Rfd::Ok => MessageDialogResult::Custom(ok.into()),
            Rfd::Cancel => MessageDialogResult::Custom(cancel.into()),
            Rfd::Custom(label) => MessageDialogResult::Custom(label),
        }
    }

    #[test]
    fn dismissals_never_read_as_approval() {
        let (ok, cancel) = (APPROVE_LABEL, CANCEL_LABEL);
        // (platform, what the person did, what rfd returns for OkCancelCustom(ok, cancel), is an approval)
        let table = [
            // macOS: first button -> Custom(first label), second -> Custom(second label), Escape and anything else -> Cancel.
            ("macos", "click first", Rfd::Custom(ok.into()), true),
            ("macos", "click second", Rfd::Custom(cancel.into()), false),
            ("macos", "escape", Rfd::Cancel, false),
            ("macos", "other modal response", Rfd::Cancel, false),
            // Windows: the OK button -> Custom(ok), the cancel button -> Custom(cancel), close box, Alt+F4, a failed call -> Cancel.
            ("windows", "click ok", Rfd::Custom(ok.into()), true),
            ("windows", "click cancel", Rfd::Custom(cancel.into()), false),
            ("windows", "close box", Rfd::Cancel, false),
            ("windows", "alt+f4", Rfd::Cancel, false),
            ("windows", "failed call", Rfd::Cancel, false),
            // Linux (GTK): rfd gives Ok or Cancel and the plugin maps them to the labels; delete-event is Cancel.
            ("linux", "click ok", Rfd::Ok, true),
            ("linux", "click cancel", Rfd::Cancel, false),
            ("linux", "close or escape", Rfd::Cancel, false),
        ];
        for (platform, event, result, expected) in table {
            let seen = plugin(result, ok, cancel);
            assert_eq!(approved(&seen), expected, "{platform}: {event}: {seen:?}");
        }
        // Whatever else could come back, only a click on the Approve label approves.
        for other in [
            MessageDialogResult::Cancel,
            MessageDialogResult::Ok,
            MessageDialogResult::Custom(CANCEL_LABEL.into()),
            MessageDialogResult::Custom(String::new()),
            MessageDialogResult::Custom(format!("{APPROVE_LABEL} ")),
        ] {
            assert!(!approved(&other), "{other:?}");
        }
    }

    #[test]
    fn invisible_characters_in_the_text_are_refused() {
        for c in [
            '\u{0}',
            '\u{7}',
            '\u{1B}',
            '\u{85}',
            '\t',
            '\n',
            '\u{00AD}',
            '\u{200B}',
            '\u{200E}',
            '\u{200F}',
            '\u{2028}',
            '\u{2029}',
            '\u{202A}',
            '\u{202E}',
            '\u{2066}',
            '\u{2067}',
            '\u{2069}',
            '\u{2060}',
            '\u{3164}',
            '\u{FE0F}',
            '\u{FEFF}',
            '\u{E0041}',
            '\u{FFA0}',
            '\u{0600}',
            '\u{0605}',
            '\u{06DD}',
            '\u{070F}',
            '\u{08E2}',
            '\u{13430}',
            '\u{1343F}',
        ] {
            for bad in [
                Hold {
                    command_shown: format!("a{c}b"),
                    ..hold()
                },
                Hold {
                    worker_name: format!("a{c}b"),
                    ..hold()
                },
                Hold {
                    task_title: format!("a{c}b"),
                    ..hold()
                },
            ] {
                assert!(!bad.is_showable(), "U+{:04X}", c as u32);
            }
        }
        // The service's visible marks and other scripts are fine.
        let ok = Hold {
            command_shown: "echo \u{23CE}\u{FFFD} \u{65E5}\u{672C} \u{1F680}".into(),
            ..hold()
        };
        assert!(ok.is_showable());
    }

    #[test]
    fn a_dialog_nobody_answers_ends_the_approval_but_not_the_dialog() {
        let presence = Presence::default();
        let flag = presence.dialog_flag();
        let (release, held) = channel::<()>();
        let result = ask_with_limit(&flag, Duration::from_millis(30), move || {
            let _ = held.recv();
            true
        });
        // The late click is thrown away: the person's wait ended in a timeout, not an approval.
        assert_eq!(result, Err(PresenceError::Timeout));
        // The dialog is still up: nothing new starts, and no second dialog opens.
        assert_eq!(presence.begin().err(), Some(PresenceError::Busy));
        assert_eq!(
            ask_with_limit(&flag, SHORT, || true),
            Err(PresenceError::Busy)
        );
        // Once it is gone, the next approval can.
        drop(release);
        let mut free = false;
        for _ in 0..100 {
            if presence.begin().is_ok() {
                free = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(free);
        assert_eq!(ask_with_limit(&flag, SHORT, || true), Ok(true));
    }

    #[test]
    fn a_percent_sign_in_a_command_is_never_a_format() {
        for text in ["date +%s", "%n", "%.0s", "100%", "%%", "a%sb%nc", "%", ""] {
            let safe = double_percent(text);
            // Read as a printf format, every `%` must be the start of `%%`.
            let mut chars = safe.chars();
            while let Some(c) = chars.next() {
                if c == '%' {
                    assert_eq!(chars.next(), Some('%'), "{text:?} -> {safe:?}");
                }
            }
            // And the person still reads the same text once the format is applied.
            assert_eq!(safe.replace("%%", "%"), text);
        }
        let hold = Hold {
            command_shown: "date +%s; echo %n".into(),
            worker_name: "50%".into(),
            task_title: "%s".into(),
            ..hold()
        };
        let body = platform_body(dialog_body(&hold));
        if cfg!(any(target_os = "macos", target_os = "windows")) {
            assert_eq!(body, dialog_body(&hold));
        } else {
            assert_eq!(body, double_percent(&dialog_body(&hold)));
            assert!(!body.replace("%%", "").contains('%'));
        }
    }

    #[test]
    fn the_labels_differ_and_the_limit_is_generous() {
        assert_ne!(APPROVE_LABEL, CANCEL_LABEL);
        assert!(PERSON_LIMIT >= Duration::from_secs(300));
    }

    #[test]
    fn approving_returns_the_nonce_the_service_was_given_bound_to_its_own_hold() {
        let presence = Arc::new(Presence::default());
        let shell = Fake::new(&presence);
        let nonce = approve(&presence, &shell, "h-1", SHORT).unwrap();
        assert!(is_nonce(&nonce));
        let sent = locked(&shell.sent);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0]["type"], "presence_describe");
        assert_eq!(sent[0]["hold_id"], "h-1");
        let grant = &sent[1];
        assert_eq!(grant["type"], "presence_grant");
        assert_eq!(grant["nonce"], nonce.as_str());
        for (key, value) in [
            ("hold_id", "h-1"),
            ("thread_id", "w-1"),
            ("task_id", "t-1"),
            ("command_sha256", &"ab".repeat(32)),
        ] {
            assert_eq!(grant[key], value, "{key}");
        }
        assert_ne!(sent[0]["request_id"], grant["request_id"]);
        assert_eq!(presence.requests().waiting(), 0);
    }

    #[test]
    fn the_grant_takes_its_ids_from_the_service_not_the_caller() {
        let presence = Arc::new(Presence::default());
        let shell = Fake::new(&presence);
        *locked(&shell.describe) = Some(Answer::Hold(Hold {
            thread_id: "w-from-service".into(),
            ..hold()
        }));
        approve(&presence, &shell, "h-1", SHORT).unwrap();
        assert_eq!(locked(&shell.sent)[1]["thread_id"], "w-from-service");
    }

    #[test]
    fn cancel_returns_cancelled_and_sends_no_grant() {
        let presence = Arc::new(Presence::default());
        let shell = Fake::new(&presence);
        *locked(&shell.click) = Ok(false);
        assert_eq!(
            approve(&presence, &shell, "h-1", SHORT),
            Err(PresenceError::Cancelled)
        );
        let sent = locked(&shell.sent);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["type"], "presence_describe");
    }

    #[test]
    fn a_service_refusal_names_its_code_and_shows_nothing() {
        let presence = Arc::new(Presence::default());
        let shell = Fake::new(&presence);
        *locked(&shell.describe) = Some(Answer::Refused("gone".into()));
        let error = approve(&presence, &shell, "h-1", SHORT).unwrap_err();
        assert_eq!(error.code(), "presence_refused:gone");
        assert!(locked(&shell.shown).is_empty());
    }

    #[test]
    fn a_refused_grant_returns_no_nonce() {
        let presence = Arc::new(Presence::default());
        let shell = Fake::new(&presence);
        *locked(&shell.grant) = Some(Answer::Refused("mismatch".into()));
        assert_eq!(
            approve(&presence, &shell, "h-1", SHORT),
            Err(PresenceError::Refused("mismatch".into()))
        );
    }

    #[test]
    fn a_silent_service_times_out_at_either_step() {
        let presence = Arc::new(Presence::default());
        let shell = Fake::new(&presence);
        *locked(&shell.describe) = None;
        assert_eq!(
            approve(&presence, &shell, "h-1", SHORT),
            Err(PresenceError::Timeout)
        );
        assert!(locked(&shell.shown).is_empty());
        assert_eq!(presence.requests().waiting(), 0);

        let shell = Fake::new(&presence);
        *locked(&shell.grant) = None;
        assert_eq!(
            approve(&presence, &shell, "h-1", SHORT),
            Err(PresenceError::Timeout)
        );
        assert_eq!(presence.requests().waiting(), 0);
    }

    #[test]
    fn no_service_is_unavailable_and_sends_nothing() {
        let presence = Arc::new(Presence::default());
        let mut shell = Fake::new(&presence);
        shell.running = false;
        assert_eq!(
            approve(&presence, &shell, "h-1", SHORT),
            Err(PresenceError::Unavailable)
        );
        assert!(locked(&shell.sent).is_empty());
    }

    #[test]
    fn an_answer_for_another_hold_or_with_control_characters_is_refused() {
        let presence = Arc::new(Presence::default());
        for bad in [
            Hold {
                hold_id: "h-2".into(),
                ..hold()
            },
            Hold {
                command_shown: "ls\nWorker: someone else".into(),
                ..hold()
            },
            Hold {
                task_title: "x\u{1b}[2J".into(),
                ..hold()
            },
            Hold {
                command_sha256: "not hex".into(),
                ..hold()
            },
            Hold {
                command_shown: String::new(),
                ..hold()
            },
            Hold {
                command_shown: "x".repeat(COMMAND_LIMIT + 1),
                ..hold()
            },
        ] {
            let shell = Fake::new(&presence);
            *locked(&shell.describe) = Some(Answer::Hold(bad));
            assert_eq!(
                approve(&presence, &shell, "h-1", SHORT),
                Err(PresenceError::InvalidAnswer)
            );
            assert!(locked(&shell.shown).is_empty());
        }
        // The wrong kind of answer for a request is refused too.
        let shell = Fake::new(&presence);
        *locked(&shell.describe) = Some(Answer::Ready);
        assert_eq!(
            approve(&presence, &shell, "h-1", SHORT),
            Err(PresenceError::InvalidAnswer)
        );
    }

    #[test]
    fn a_bad_hold_id_never_reaches_the_service() {
        let presence = Arc::new(Presence::default());
        let shell = Fake::new(&presence);
        for id in [
            "",
            "a b",
            "../x",
            "h\n1",
            &"h".repeat(ID_LIMIT + 1),
            "h\u{e9}",
        ] {
            assert_eq!(
                approve(&presence, &shell, id, SHORT),
                Err(PresenceError::InvalidHold)
            );
        }
        assert!(locked(&shell.sent).is_empty());
    }

    #[test]
    fn a_second_approval_while_one_is_open_is_busy_and_the_next_one_can_start_after() {
        let presence = Arc::new(Presence::default());
        let first = presence.begin().unwrap();
        let shell = Fake::new(&presence);
        assert_eq!(
            approve(&presence, &shell, "h-1", SHORT),
            Err(PresenceError::Busy)
        );
        assert!(locked(&shell.sent).is_empty());
        drop(first);
        // A cancelled approval frees the next one too.
        *locked(&shell.click) = Ok(false);
        assert_eq!(
            approve(&presence, &shell, "h-1", SHORT),
            Err(PresenceError::Cancelled)
        );
        assert!(presence.begin().is_ok());
    }

    #[test]
    fn concurrent_begins_have_exactly_one_winner() {
        let presence = Arc::new(Presence::default());
        let gate = Arc::new(std::sync::Barrier::new(2));
        let runs: Vec<_> = (0..2)
            .map(|_| {
                let (presence, gate) = (presence.clone(), gate.clone());
                std::thread::spawn(move || {
                    let flight = presence.begin();
                    // Neither lets go until both have tried.
                    gate.wait();
                    flight.map(|_| ())
                })
            })
            .collect();
        let results: Vec<_> = runs.into_iter().map(|run| run.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        assert!(results.contains(&Err(PresenceError::Busy)));
    }

    #[test]
    fn an_answer_is_matched_by_request_id_and_a_stranger_is_ignored() {
        let requests = Requests::default();
        let one = requests.open();
        let two = requests.open();
        assert_ne!(one.id(), two.id());
        assert!(!requests.deliver("presence-99", Answer::Ready));
        assert!(!requests.deliver("", Answer::Ready));
        // Answered out of order, each reaches its own request.
        assert!(requests.deliver(two.id(), Answer::Refused("gone".into())));
        assert!(requests.deliver(one.id(), Answer::Ready));
        assert_eq!(one.wait(SHORT), Ok(Answer::Ready));
        assert_eq!(two.wait(SHORT), Ok(Answer::Refused("gone".into())));
        assert_eq!(requests.waiting(), 0);
    }

    #[test]
    fn a_late_or_repeated_answer_finds_no_request() {
        let requests = Requests::default();
        let pending = requests.open();
        let id = pending.id().to_owned();
        assert_eq!(
            pending.wait(Duration::from_millis(10)),
            Err(PresenceError::Timeout)
        );
        assert!(!requests.deliver(&id, Answer::Ready));
        let again = requests.open();
        let id = again.id().to_owned();
        assert!(requests.deliver(&id, Answer::Ready));
        assert!(!requests.deliver(&id, Answer::Ready));
    }

    #[test]
    fn request_ids_are_ids_the_service_accepts() {
        let requests = Requests::default();
        for _ in 0..5 {
            assert!(valid_id(requests.open().id()));
        }
    }

    #[test]
    fn the_nonce_is_32_lowercase_hex_and_never_repeats() {
        let first = Nonce::mint().unwrap();
        let second = Nonce::mint().unwrap();
        assert!(is_nonce(first.as_str()));
        assert!(is_nonce(second.as_str()));
        assert_ne!(first.as_str(), second.as_str());
        // The encoding, checked against bytes made now: each byte is two lowercase hex digits, in order.
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).unwrap();
        let expected: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(Nonce::from_bytes(&bytes).as_str(), expected);
    }

    #[test]
    fn a_nonce_shows_nothing_of_itself_and_is_wiped_when_dropped() {
        let nonce = Nonce::mint().unwrap();
        let text = nonce.as_str().to_owned();
        assert!(!format!("{nonce:?}").contains(&text));
        let mut buffer = [0u8; 32];
        getrandom::fill(&mut buffer).unwrap();
        wipe(&mut buffer);
        assert_eq!(buffer, [0u8; 32]);
    }

    #[test]
    fn no_error_can_show_a_nonce() {
        let nonce = Nonce::mint().unwrap();
        let text = nonce.as_str().to_owned();
        for error in [
            PresenceError::InvalidHold,
            PresenceError::Forbidden,
            PresenceError::Busy,
            PresenceError::Unavailable,
            PresenceError::Timeout,
            PresenceError::Cancelled,
            PresenceError::refused("gone"),
            PresenceError::refused(&text),
            PresenceError::InvalidAnswer,
            PresenceError::Dialog,
            PresenceError::Nonce,
        ] {
            for shown in [
                format!("{error}"),
                format!("{error:?}"),
                serde_json::to_string(&error).unwrap(),
            ] {
                assert!(!shown.contains(&text), "{shown}");
            }
        }
    }

    #[test]
    fn codes_are_stable_and_a_strange_service_code_becomes_unknown() {
        assert_eq!(PresenceError::Cancelled.code(), "cancelled");
        assert_eq!(PresenceError::Busy.code(), "busy");
        assert_eq!(PresenceError::Timeout.code(), "timeout");
        assert_eq!(
            PresenceError::refused("not_found").code(),
            "presence_refused:not_found"
        );
        for code in [
            "",
            "Gone",
            "gone now",
            "a:b",
            "<b>",
            &"x".repeat(CODE_LIMIT + 1),
        ] {
            assert_eq!(
                PresenceError::refused(code).code(),
                "presence_refused:unknown"
            );
        }
        // The page gets the code as a plain string.
        assert_eq!(
            serde_json::to_value(PresenceError::Busy).unwrap(),
            json!("busy")
        );
    }

    #[test]
    fn the_dialog_says_exactly_what_the_designer_wrote() {
        assert_eq!(DIALOG_TITLE, "Approve this command once?");
        assert_eq!(
            dialog_body(&hold()),
            "make deploy --to prod\n\nWorker: Maya\nTask: Ship the release\n\nThis approves this one command. It cannot be reused."
        );
        assert_eq!(APPROVE_LABEL, "Approve once");
        assert_eq!(CANCEL_LABEL, "Cancel");
    }

    #[test]
    fn a_cut_command_ends_with_an_ellipsis_and_a_whole_one_does_not() {
        let cut = dialog_body(&Hold {
            truncated: true,
            ..hold()
        });
        assert!(cut.starts_with("make deploy --to prod\u{2026}\n\nWorker:"));
        assert!(!dialog_body(&hold()).contains('\u{2026}'));
    }

    #[test]
    fn the_body_is_plain_text_nothing_is_escaped_or_interpreted() {
        let text = Hold {
            command_shown: "echo <b>x</b> &amp; [a](http://e.test) `y` %s {0} \\n".into(),
            worker_name: "<i>Maya</i>".into(),
            task_title: "&lt;t&gt; **bold**".into(),
            ..hold()
        };
        assert_eq!(
            dialog_body(&text),
            "echo <b>x</b> &amp; [a](http://e.test) `y` %s {0} \\n\n\nWorker: <i>Maya</i>\nTask: &lt;t&gt; **bold**\n\nThis approves this one command. It cannot be reused."
        );
    }

    #[test]
    fn the_dialog_never_makes_a_forbidden_claim_or_names_a_time() {
        let body = dialog_body(&hold()).to_lowercase();
        let text =
            format!("{body} {} {} {}", DIALOG_TITLE, APPROVE_LABEL, CANCEL_LABEL).to_lowercase();
        for word in [
            "secure", "tamper", "sandbox", "isolated", "only", "minute", "second", "expire", "time",
        ] {
            assert!(!text.contains(word), "{word}");
        }
    }

    #[test]
    fn only_a_click_on_approve_once_counts() {
        assert!(approved(&MessageDialogResult::Custom(APPROVE_LABEL.into())));
        for other in [
            MessageDialogResult::Custom(CANCEL_LABEL.into()),
            MessageDialogResult::Cancel,
            MessageDialogResult::Ok,
            MessageDialogResult::Yes,
            MessageDialogResult::No,
            MessageDialogResult::default(),
        ] {
            assert!(!approved(&other));
        }
    }

    #[test]
    fn the_lines_to_the_service_have_the_agreed_shape() {
        assert_eq!(
            describe_line("r1", "h-1"),
            json!({ "type": "presence_describe", "request_id": "r1", "hold_id": "h-1" })
        );
        let nonce = Nonce::mint().unwrap();
        assert_eq!(
            grant_line("r2", &nonce, &hold()),
            json!({
                "type": "presence_grant",
                "request_id": "r2",
                "nonce": nonce.as_str(),
                "hold_id": "h-1",
                "thread_id": "w-1",
                "task_id": "t-1",
                "command_sha256": "ab".repeat(32),
            })
        );
    }

    #[test]
    fn the_service_answers_are_read() {
        use crate::core_process::ServiceMessage as M;
        let read = |value: Value| serde_json::from_value::<M>(value).unwrap();
        assert!(matches!(
            read(json!({ "type": "presence_ready", "request_id": "r1" })),
            M::PresenceReady { request_id } if request_id == "r1"
        ));
        assert!(matches!(
            read(json!({ "type": "presence_refused", "request_id": "r1", "code": "gone" })),
            M::PresenceRefused { request_id, code } if request_id == "r1" && code == "gone"
        ));
        let held = read(json!({
            "type": "presence_hold", "request_id": "r1", "hold_id": "h-1", "thread_id": "w-1",
            "task_id": "t-1", "command_shown": "ls", "command_sha256": "ab", "worker_name": "Maya",
            "task_title": "T", "truncated": true,
        }));
        assert!(
            matches!(held, M::PresenceHold { truncated: true, ref command_shown, .. } if command_shown == "ls")
        );
        // An older service that says nothing about a cut means not cut.
        let older = read(json!({
            "type": "presence_hold", "request_id": "r1", "hold_id": "h-1", "thread_id": "w-1",
            "task_id": "t-1", "command_shown": "ls", "command_sha256": "ab", "worker_name": "Maya",
            "task_title": "T",
        }));
        assert!(matches!(
            older,
            M::PresenceHold {
                truncated: false,
                ..
            }
        ));
    }
}
