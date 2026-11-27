//! W10: an engine console that stopped and waits for the person.
//!
//! The console's pty output is read in memory, cleaned and redacted, and only then
//! appended to the thread's own event log as `CUSTOM orbyn.terminal_attention`. The
//! wait is quiet time plus a question-like last line, or the pty's no-echo mode.
//! Nothing here writes to a terminal, and no answer route exists.

pub(crate) mod excerpt;
pub(crate) mod heuristic;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use orbyn_contracts::{
    AgUiEvent, TerminalAttentionClearReason, TerminalAttentionEvent, TerminalAttentionKind,
    TerminalInfo,
};
use serde_json::Value;

use crate::app::Core;
use crate::error::Result;
use crate::host::shell::ShellBridge;
use crate::host::terminal::{PtyOutputState, PtySession};
use crate::redaction::{Redactor, Surface};
use crate::store::Store;

/// The AG-UI `CUSTOM` event name on the thread's event log.
pub(crate) const EVENT_NAME: &str = "orbyn.terminal_attention";
/// The device setting that turns detection on, on by default.
pub(crate) const ENABLED_PREFERENCE: &str = "terminal_attention_enabled";
/// The device setting for the quiet time, in seconds.
pub(crate) const QUIET_PREFERENCE: &str = "terminal_attention_quiet_seconds";
pub(crate) const QUIET_DEFAULT_SECS: u64 = 20;
pub(crate) const QUIET_MIN_SECS: u64 = 10;
pub(crate) const QUIET_MAX_SECS: u64 = 120;

/// A service-made id is opaque and never reused; a ULID is 26 characters.
const MAX_ID_CHARS: usize = 64;
/// The whole event value stays under 4 KiB.
const MAX_VALUE_BYTES: usize = 4096;
/// A waiting item is updated at most once in this many milliseconds.
const UPDATE_EVERY_MS: i64 = 5_000;
/// A new episode on one terminal needs at least this long since the last waiting.
const EPISODE_GAP_MS: i64 = 10_000;
const MAX_PER_THREAD: usize = 8;
const MAX_PER_SERVICE: usize = 32;
/// How often a quiet console is checked. Output, input and the end are events and
/// do not wait for this tick.
const TICK: Duration = Duration::from_secs(1);

/// A cleaned, redacted reading of the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Evaluation {
    pub kind: TerminalAttentionKind,
    pub excerpt: Vec<String>,
}

/// What the state machine asks to be appended to a thread's log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Notice {
    Waiting {
        thread_id: String,
        terminal_id: String,
        id: String,
        kind: TerminalAttentionKind,
        since: String,
        excerpt: Vec<String>,
    },
    Clear {
        thread_id: String,
        id: String,
        reason: TerminalAttentionClearReason,
    },
}

impl Notice {
    fn thread_id(&self) -> &str {
        match self {
            Self::Waiting { thread_id, .. } | Self::Clear { thread_id, .. } => thread_id,
        }
    }

    fn terminal_id(&self) -> Option<&str> {
        match self {
            Self::Waiting { terminal_id, .. } => Some(terminal_id),
            Self::Clear { .. } => None,
        }
    }

    /// The version-1 event value. A waiting item is cut further if its fields and
    /// excerpt would cross the 4 KiB budget.
    fn value(&self, terminal_title: &str, engine: &str) -> TerminalAttentionEvent {
        match self {
            Self::Waiting {
                id,
                terminal_id,
                kind,
                since,
                excerpt,
                ..
            } => waiting_value(
                id,
                terminal_id,
                terminal_title,
                engine,
                *kind,
                since,
                excerpt,
            ),
            Self::Clear { id, reason, .. } => TerminalAttentionEvent::Clear {
                v: 1,
                id: id.clone(),
                reason: *reason,
            },
        }
    }
}

fn waiting_value(
    id: &str,
    terminal_id: &str,
    terminal_title: &str,
    engine: &str,
    kind: TerminalAttentionKind,
    since: &str,
    lines: &[String],
) -> TerminalAttentionEvent {
    let terminal_title = excerpt::one_line(terminal_title, excerpt::MAX_TITLE_CHARS);
    let engine = excerpt::one_line(engine, excerpt::MAX_ENGINE_CHARS);
    // The state machine already capped these; cap again here, so every value that
    // leaves has lines of at most 200 characters whatever built the notice.
    let mut excerpt: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        excerpt.push(excerpt::cut_line(line));
    }
    loop {
        let event = TerminalAttentionEvent::Waiting {
            v: 1,
            id: id.to_owned(),
            terminal_id: terminal_id.to_owned(),
            terminal_title: terminal_title.clone(),
            engine: engine.clone(),
            kind,
            since: since.to_owned(),
            excerpt: excerpt.clone(),
        };
        if value_bytes(&event) < MAX_VALUE_BYTES || excerpt.is_empty() {
            return event;
        }
        // The newest lines stay; the oldest one goes first.
        excerpt.remove(0);
    }
}

fn value_bytes(value: &TerminalAttentionEvent) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

/// Reads a terminal's text and returns what it waits for, if it does. A secret is
/// decided by the terminal's mode, never by the words; its excerpt is only the
/// prompt label, capped, and never what the person typed.
pub(crate) fn evaluate(redactor: &Redactor, text: &str, echo_off: bool) -> Option<Evaluation> {
    let plain = excerpt::plain(text);
    if plain.trim().is_empty() {
        return None;
    }
    if echo_off {
        let label = excerpt::secret_label(&plain).unwrap_or_default();
        let label = excerpt::one_line(&redactor.labelled(&label), excerpt::MAX_LABEL_CHARS);
        return Some(Evaluation {
            kind: TerminalAttentionKind::Secret,
            excerpt: if label.is_empty() {
                Vec::new()
            } else {
                vec![label]
            },
        });
    }
    let kind = heuristic::classify_plain(&plain)?;
    Some(Evaluation {
        kind,
        excerpt: excerpt::lines(&redactor.labelled(&plain)),
    })
}

/// The quiet setting's seconds: 10 to 120, and 20 when the stored value is not.
pub(crate) fn quiet_seconds(value: &Value) -> u64 {
    value
        .as_u64()
        .filter(|seconds| (QUIET_MIN_SECS..=QUIET_MAX_SECS).contains(seconds))
        .unwrap_or(QUIET_DEFAULT_SECS)
}

/// Whether the setting is on. The default is on, and anything else but `false` reads on.
pub(crate) fn enabled(value: &Value) -> bool {
    value.as_bool().unwrap_or(true)
}

/// Whether detection is on now. It is read from the store, not from the console's
/// start, so turning the setting off stops new items on a console that is already
/// open (#1764). A setting that cannot be read counts as off, as at the console's open.
fn detection_on(store: &Store) -> bool {
    crate::settings::preferences::stored_preference(store, ENABLED_PREFERENCE, None)
        .is_ok_and(|value| enabled(&value))
}

/// Appends one notice to its thread's log and tells the windows where the log grew.
/// The excerpt is already redacted. Nothing logs the text: the line names the
/// terminal and the event's byte count.
pub(crate) fn append(
    store: &Store,
    shell: &ShellBridge,
    notice: &Notice,
    terminal_title: &str,
    engine: &str,
) -> Result<()> {
    let value = notice.value(terminal_title, engine);
    let bytes = value_bytes(&value);
    let event = AgUiEvent::Custom {
        name: EVENT_NAME.to_owned(),
        value: serde_json::to_value(&value)?,
    };
    let appended = store.append_thread_event(notice.thread_id(), &event)?;
    shell.thread_event(notice.thread_id(), appended.seq);
    if let Some(terminal_id) = notice.terminal_id() {
        tracing::debug!(
            terminal = %terminal_id,
            bytes,
            "terminal attention event appended"
        );
    }
    Ok(())
}

fn emit_all(
    store: &Store,
    shell: &ShellBridge,
    notices: Vec<Notice>,
    terminal_title: &str,
    engine: &str,
) {
    for notice in notices {
        if let Err(error) = append(store, shell, &notice, terminal_title, engine) {
            tracing::warn!(%error, "could not append a terminal attention event");
        }
    }
}

type Key = (String, String);

#[derive(Default)]
struct Episode {
    active: Option<Active>,
    /// When the last waiting episode started, so a new one keeps its distance even
    /// after a clear.
    last_waiting_ms: Option<i64>,
}

struct Active {
    id: String,
    kind: TerminalAttentionKind,
    since: String,
    excerpt: Vec<String>,
    /// The episode's start, which the limits use to find the oldest item.
    waiting_ms: i64,
    /// When this item was last sent, so updates are at most one every 5 s.
    sent_ms: i64,
    order: u64,
}

/// The per-thread and per-service state: one active item per terminal, the episode
/// gap, the throttle and the two caps. Times are milliseconds from any fixed origin;
/// every transition takes the clock it should read.
#[derive(Default)]
pub(crate) struct Registry {
    episodes: HashMap<Key, Episode>,
    order: u64,
}

impl Registry {
    /// A quiet terminal's reading. It starts an episode, updates the active item with
    /// the same id, or does nothing while the gap or the throttle says so.
    pub(crate) fn quiet(
        &mut self,
        thread_id: &str,
        terminal_id: &str,
        now_ms: i64,
        since: String,
        evaluation: Evaluation,
        make_id: &mut dyn FnMut() -> String,
    ) -> Vec<Notice> {
        let key = (thread_id.to_owned(), terminal_id.to_owned());
        let mut episode = self.episodes.remove(&key).unwrap_or_default();
        if let Some(active) = episode.active.as_mut() {
            let changed = active.kind != evaluation.kind
                || active.since != since
                || active.excerpt != evaluation.excerpt;
            if changed && now_ms.saturating_sub(active.sent_ms) >= UPDATE_EVERY_MS {
                active.kind = evaluation.kind;
                active.since = since;
                active.excerpt = evaluation.excerpt;
                active.sent_ms = now_ms;
                let notice = waiting_notice(thread_id, terminal_id, active);
                self.episodes.insert(key, episode);
                return vec![notice];
            }
            self.episodes.insert(key, episode);
            return Vec::new();
        }
        if episode
            .last_waiting_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < EPISODE_GAP_MS)
        {
            self.episodes.insert(key, episode);
            return Vec::new();
        }
        let id = make_id();
        if id.is_empty() || id.len() > MAX_ID_CHARS {
            self.episodes.insert(key, episode);
            return Vec::new();
        }
        let mut notices = self.make_room(thread_id);
        self.order = self.order.wrapping_add(1);
        episode.last_waiting_ms = Some(now_ms);
        episode.active = Some(Active {
            id,
            kind: evaluation.kind,
            since,
            excerpt: evaluation.excerpt,
            waiting_ms: now_ms,
            sent_ms: now_ms,
            order: self.order,
        });
        if let Some(active) = episode.active.as_ref() {
            notices.push(waiting_notice(thread_id, terminal_id, active));
        }
        self.episodes.insert(key, episode);
        notices
    }

    /// The person wrote to the terminal. The bytes are never read.
    pub(crate) fn input(&mut self, thread_id: &str, terminal_id: &str) -> Vec<Notice> {
        self.clear(thread_id, terminal_id, TerminalAttentionClearReason::Input)
    }

    /// The terminal printed again.
    pub(crate) fn output(&mut self, thread_id: &str, terminal_id: &str) -> Vec<Notice> {
        self.clear(thread_id, terminal_id, TerminalAttentionClearReason::Output)
    }

    /// The terminal ended or was closed.
    pub(crate) fn closed(&mut self, thread_id: &str, terminal_id: &str) -> Vec<Notice> {
        self.clear(thread_id, terminal_id, TerminalAttentionClearReason::Closed)
    }

    pub(crate) fn forget(&mut self, thread_id: &str, terminal_id: &str) {
        self.episodes
            .remove(&(thread_id.to_owned(), terminal_id.to_owned()));
    }

    /// The items waiting now in one thread, oldest first, as the state machine
    /// holds them: already redacted, never re-read from a terminal.
    pub(crate) fn open_items(&self, thread_id: &str) -> Vec<Notice> {
        let mut open: Vec<(i64, u64, Notice)> = self
            .episodes
            .iter()
            .filter(|(key, _)| key.0 == thread_id)
            .filter_map(|(key, episode)| {
                let active = episode.active.as_ref()?;
                Some((
                    active.waiting_ms,
                    active.order,
                    waiting_notice(&key.0, &key.1, active),
                ))
            })
            .collect();
        open.sort_by_key(|(waiting_ms, order, _)| (*waiting_ms, *order));
        open.into_iter().map(|(_, _, notice)| notice).collect()
    }

    fn clear(
        &mut self,
        thread_id: &str,
        terminal_id: &str,
        reason: TerminalAttentionClearReason,
    ) -> Vec<Notice> {
        let key = (thread_id.to_owned(), terminal_id.to_owned());
        let Some(episode) = self.episodes.get_mut(&key) else {
            return Vec::new();
        };
        let Some(active) = episode.active.take() else {
            return Vec::new();
        };
        vec![Notice::Clear {
            thread_id: thread_id.to_owned(),
            id: active.id,
            reason,
        }]
    }

    /// Drops the oldest items until the new one fits under both caps.
    fn make_room(&mut self, thread_id: &str) -> Vec<Notice> {
        let mut notices = Vec::new();
        while self.active_in_thread(thread_id) >= MAX_PER_THREAD {
            let Some(key) = self.oldest_active(Some(thread_id)) else {
                break;
            };
            if let Some(notice) = self.remove_active(&key) {
                notices.push(notice);
            }
        }
        while self.active_count() >= MAX_PER_SERVICE {
            let Some(key) = self.oldest_active(None) else {
                break;
            };
            if let Some(notice) = self.remove_active(&key) {
                notices.push(notice);
            }
        }
        notices
    }

    fn remove_active(&mut self, key: &Key) -> Option<Notice> {
        let episode = self.episodes.get_mut(key)?;
        let active = episode.active.take()?;
        Some(Notice::Clear {
            thread_id: key.0.clone(),
            id: active.id,
            reason: TerminalAttentionClearReason::Closed,
        })
    }

    fn active_count(&self) -> usize {
        self.episodes
            .values()
            .filter(|episode| episode.active.is_some())
            .count()
    }

    fn active_in_thread(&self, thread_id: &str) -> usize {
        self.episodes
            .iter()
            .filter(|(key, episode)| key.0 == thread_id && episode.active.is_some())
            .count()
    }

    fn oldest_active(&self, thread_id: Option<&str>) -> Option<Key> {
        self.episodes
            .iter()
            .filter(|(key, episode)| {
                episode.active.is_some() && thread_id.is_none_or(|wanted| key.0 == wanted)
            })
            .min_by_key(|(_, episode)| {
                episode
                    .active
                    .as_ref()
                    .map_or((i64::MAX, u64::MAX), |active| {
                        (active.waiting_ms, active.order)
                    })
            })
            .map(|(key, _)| (key.0.clone(), key.1.clone()))
    }
}

fn waiting_notice(thread_id: &str, terminal_id: &str, active: &Active) -> Notice {
    Notice::Waiting {
        thread_id: thread_id.to_owned(),
        terminal_id: terminal_id.to_owned(),
        id: active.id.clone(),
        kind: active.kind,
        since: active.since.clone(),
        excerpt: active.excerpt.clone(),
    }
}

/// The service-wide state and the one watcher per engine console.
#[derive(Default)]
pub(crate) struct AttentionHub {
    registry: Mutex<Registry>,
    watching: Mutex<HashMap<Key, String>>,
}

impl AttentionHub {
    /// Takes the watcher slot for a terminal's process. The creation stamp tells a
    /// reopened terminal from the one that just ended.
    pub(crate) fn claim(&self, thread_id: &str, terminal_id: &str, created_at: &str) -> bool {
        let key = (thread_id.to_owned(), terminal_id.to_owned());
        let mut watching = lock(&self.watching);
        if watching.get(&key).is_some_and(|known| known == created_at) {
            return false;
        }
        watching.insert(key, created_at.to_owned());
        true
    }

    /// Frees the watcher slot and forgets the terminal's state. A newer process that
    /// already claimed the slot keeps its own state.
    pub(crate) fn release(&self, thread_id: &str, terminal_id: &str, created_at: &str) {
        let key = (thread_id.to_owned(), terminal_id.to_owned());
        let removed = {
            let mut watching = lock(&self.watching);
            let ours = watching.get(&key).is_some_and(|known| known == created_at);
            if ours {
                let _ = watching.remove(&key);
            }
            ours
        };
        if removed {
            lock(&self.registry).forget(thread_id, terminal_id);
        }
    }

    pub(crate) fn quiet(
        &self,
        thread_id: &str,
        terminal_id: &str,
        now_ms: i64,
        since: String,
        evaluation: Evaluation,
    ) -> Vec<Notice> {
        let mut make_id = crate::util::new_id;
        lock(&self.registry).quiet(
            thread_id,
            terminal_id,
            now_ms,
            since,
            evaluation,
            &mut make_id,
        )
    }

    pub(crate) fn input(&self, thread_id: &str, terminal_id: &str) -> Vec<Notice> {
        lock(&self.registry).input(thread_id, terminal_id)
    }

    pub(crate) fn output(&self, thread_id: &str, terminal_id: &str) -> Vec<Notice> {
        lock(&self.registry).output(thread_id, terminal_id)
    }

    pub(crate) fn closed(&self, thread_id: &str, terminal_id: &str) -> Vec<Notice> {
        lock(&self.registry).closed(thread_id, terminal_id)
    }

    pub(crate) fn open_items(&self, thread_id: &str) -> Vec<Notice> {
        lock(&self.registry).open_items(thread_id)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One engine console's watcher.
struct Watch {
    thread_id: String,
    terminal_id: String,
    terminal_title: String,
    engine: String,
    quiet: Duration,
    redactor: Arc<Redactor>,
    hub: Arc<AttentionHub>,
    store: Arc<Store>,
    shell: Arc<ShellBridge>,
    session: Arc<PtySession>,
}

impl Core {
    /// The items waiting now in a thread: the `waiting` values its log announced and
    /// has not cleared, at most 8. Read from the registry in memory; no terminal is
    /// read and nothing is written. An unknown thread is not found.
    pub fn terminal_attention(&self, thread_id: &str) -> Result<Vec<TerminalAttentionEvent>> {
        let thread = self.store.thread(thread_id)?;
        let titles: HashMap<String, String> = self
            .terminals
            .list(thread_id)
            .into_iter()
            .map(|info| (info.id, info.title))
            .collect();
        Ok(self
            .attention
            .open_items(thread_id)
            .iter()
            .take(MAX_PER_THREAD)
            .map(|notice| {
                let title = notice
                    .terminal_id()
                    .and_then(|id| titles.get(id))
                    .map_or("", String::as_str);
                notice.value(title, &thread.engine)
            })
            .collect())
    }

    /// Starts the one watcher for the thread's engine console. It starts only while
    /// the setting is on and a redactor was obtained, and it reads the setting again
    /// each time it would report; a plain shell never comes here.
    pub(crate) fn watch_engine_console(self: &Arc<Self>, thread_id: &str, info: &TerminalInfo) {
        if !detection_on(&self.store) {
            return;
        }
        let redactor = match self.redactor(Surface::Terminal, None) {
            Ok(redactor) => redactor,
            Err(error) => {
                tracing::warn!(%error, "terminal attention stays off without a redactor");
                return;
            }
        };
        let Ok(thread) = self.store.thread(thread_id) else {
            return;
        };
        let quiet = self
            .preference(QUIET_PREFERENCE, None)
            .map_or(QUIET_DEFAULT_SECS, |value| quiet_seconds(&value));
        let Some(session) = self.terminals.named(thread_id, &info.id) else {
            return;
        };
        // A terminal id can be reused by a plain shell; only an engine console is
        // ever reported.
        if !session.engine() {
            return;
        }
        if !self.attention.claim(thread_id, &info.id, &info.created_at) {
            return;
        }
        let hub = self.attention.clone();
        let watch = Watch {
            thread_id: thread_id.to_owned(),
            terminal_id: info.id.clone(),
            terminal_title: info.title.clone(),
            engine: thread.engine,
            quiet: Duration::from_secs(quiet),
            redactor,
            hub: hub.clone(),
            store: self.store.clone(),
            shell: self.shell.clone(),
            session,
        };
        let (thread_id, terminal_id, created_at) = (
            watch.thread_id.clone(),
            watch.terminal_id.clone(),
            info.created_at.clone(),
        );
        tokio::spawn(async move {
            run(watch).await;
            hub.release(&thread_id, &terminal_id, &created_at);
        });
    }
}

async fn run(watch: Watch) {
    let Watch {
        thread_id,
        terminal_id,
        terminal_title,
        engine,
        quiet,
        redactor,
        hub,
        store,
        shell,
        session,
    } = watch;
    let (scrollback, mut output) = session.attach();
    let mut writes = session.writes();
    // A second receiver, so the output branch can ask whether a write is already
    // pending without borrowing the receiver its own branch is waiting on.
    let mut written = session.writes();
    let mut state = session.output_state();
    let mut exit = session.exited();
    // A session that ended before this watcher could subscribe has nothing to watch.
    if exit.borrow().is_some() || *state.borrow() != PtyOutputState::Reading {
        return;
    }
    // Output already in the scrollback happened before this watcher: it counts as
    // the last print, so only the quiet after it is watched.
    let mut printed = !scrollback.is_empty();
    let mut last_output = Instant::now();
    let mut since = crate::util::now();
    let started = Instant::now();
    let mut ticks = tokio::time::interval(TICK);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            received = output.recv() => match received {
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // A write that has not reached its own branch yet still wins over
                    // the echo it causes: the person answered, so the clear says input.
                    let wrote = written.has_changed().unwrap_or(false);
                    if wrote {
                        let _ = written.borrow_and_update();
                    }
                    let notices = if wrote {
                        hub.input(&thread_id, &terminal_id)
                    } else {
                        hub.output(&thread_id, &terminal_id)
                    };
                    emit_all(&store, &shell, notices, &terminal_title, &engine);
                    printed = true;
                    last_output = Instant::now();
                    since = crate::util::now();
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    emit_all(
                        &store,
                        &shell,
                        hub.closed(&thread_id, &terminal_id),
                        &terminal_title,
                        &engine,
                    );
                    return;
                }
            },
            changed = writes.changed() => {
                if changed.is_err() {
                    emit_all(
                        &store,
                        &shell,
                        hub.closed(&thread_id, &terminal_id),
                        &terminal_title,
                        &engine,
                    );
                    return;
                }
                emit_all(
                    &store,
                    &shell,
                    hub.input(&thread_id, &terminal_id),
                    &terminal_title,
                    &engine,
                );
            }
            changed = state.changed() => {
                if changed.is_err() || *state.borrow() != PtyOutputState::Reading {
                    emit_all(
                        &store,
                        &shell,
                        hub.closed(&thread_id, &terminal_id),
                        &terminal_title,
                        &engine,
                    );
                    return;
                }
            }
            changed = exit.changed() => {
                // The child ending is `closed` even if an inherited descriptor keeps
                // its output open for a while.
                if changed.is_err() || exit.borrow().is_some() {
                    emit_all(
                        &store,
                        &shell,
                        hub.closed(&thread_id, &terminal_id),
                        &terminal_title,
                        &engine,
                    );
                    return;
                }
            }
            _ = ticks.tick() => {
                if !printed || last_output.elapsed() < quiet {
                    continue;
                }
                // The setting can be turned off while this console stays open, so it is
                // read again here, where a report would be made (#1764). A clear still
                // follows the terminal's own input, output or end.
                if !detection_on(&store) {
                    continue;
                }
                let text =
                    excerpt::tail_text(&session.scrollback_tail(excerpt::TAIL_BYTES), excerpt::TAIL_BYTES);
                let Some(evaluation) = evaluate(&redactor, &text, session.echo_off()) else {
                    continue;
                };
                let notices = hub.quiet(
                    &thread_id,
                    &terminal_id,
                    now_ms(started),
                    since.clone(),
                    evaluation,
                );
                emit_all(&store, &shell, notices, &terminal_title, &engine);
            }
        }
    }
}

fn now_ms(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests;
