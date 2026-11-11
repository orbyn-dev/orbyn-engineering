//! Answers served from the last known value while a fresher one is read in the
//! background (stale-while-revalidate).
//!
//! The desktop webview holds a handful of connections to this service, so a request
//! that waits on a forge, an engine or Git for seconds starves the quick ones behind
//! it. A [`Swr`] keeps each answer by key. With a value kept, a call returns it at
//! once; when the value has passed its fresh time, the same call starts one refresh
//! that every other caller shares, and tells the windows through [`Notify`] when the
//! refresh changed the answer. Only a call with nothing kept waits for a read, and it
//! shares a read already running.
//!
//! A key must carry every input of the read (project, thread, engine, query, state):
//! a value is never served for another key. A failed refresh keeps the last good
//! value, and a call with nothing kept gets the read's own error.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::error::{CoreError, Result};

/// Tells the windows that the data behind a topic changed, so they read it again.
pub type Notify = Arc<dyn Fn(&str) + Send + Sync>;

/// After a refresh of a kept value fails, none is started again for this long, so a
/// broken forge or engine is not asked on every request.
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(5);
const SEPARATOR: char = '\u{1f}';

/// A key made of every input of a read. Each part ends with a separator, so the key of
/// pull request 1 is never a prefix of the key of pull request 12.
pub fn key(parts: &[&str]) -> String {
    let mut key = String::new();
    for part in parts {
        key.push_str(part);
        key.push(SEPARATOR);
    }
    key
}

type Answer<V> = Arc<std::result::Result<V, CoreError>>;
type Flight<V> = watch::Receiver<Option<Answer<V>>>;

struct Entry<V> {
    value: Option<V>,
    /// When `value` was read.
    at: Instant,
    touched: Instant,
    retry_after: Option<Instant>,
    /// The read running for this key, by id.
    flight: Option<(u64, Flight<V>)>,
    /// A read may store its answer only when its id is at least this. Starting a newer
    /// read, storing an answer or dropping the value raises it, so an older read that
    /// finishes late cannot put old data back.
    floor: u64,
}

struct State<V> {
    entries: HashMap<String, Entry<V>>,
    next: u64,
}

pub struct Swr<V> {
    /// How long a value is served without starting a refresh.
    fresh: Duration,
    /// How long a value may be served at all; older, a call waits for a read.
    max_age: Duration,
    /// Most keys kept. The least recently used key without a running read goes first.
    cap: usize,
    /// Whether an answer is worth keeping. One that is not (an engine's stand-in list
    /// after a failure) still goes to the callers waiting on it, but the kept value stays.
    keep: fn(&V) -> bool,
    /// Whether a fresh answer says what the kept one did, so nothing is announced.
    same: fn(&V, &V) -> bool,
    notify: OnceLock<Notify>,
    state: Arc<Mutex<State<V>>>,
}

fn lock<V>(state: &Mutex<State<V>>) -> MutexGuard<'_, State<V>> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Clears a key's running read when its task ends, however it ends.
struct FlightGuard<V> {
    state: Arc<Mutex<State<V>>>,
    key: String,
    id: u64,
}

impl<V> Drop for FlightGuard<V> {
    fn drop(&mut self) {
        if let Some(entry) = lock(&self.state).entries.get_mut(&self.key)
            && entry.flight.as_ref().is_some_and(|(id, _)| *id == self.id)
        {
            entry.flight = None;
        }
    }
}

enum Begun<V> {
    Answer(V),
    Wait(Flight<V>),
}

impl<V: Clone + PartialEq + Send + Sync + 'static> Swr<V> {
    pub fn new(fresh: Duration, max_age: Duration, cap: usize) -> Self {
        Self {
            fresh,
            max_age,
            cap: cap.max(1),
            keep: |_| true,
            same: |old, new| old == new,
            notify: OnceLock::new(),
            state: Arc::new(Mutex::new(State {
                entries: HashMap::new(),
                next: 0,
            })),
        }
    }

    /// Keeps only the answers `keep` accepts.
    #[must_use]
    pub fn keeping(mut self, keep: fn(&V) -> bool) -> Self {
        self.keep = keep;
        self
    }

    /// Decides when two answers are the same: a read time or a measured latency that
    /// differs on every read is not a change a window needs to hear about.
    #[must_use]
    pub fn same_as(mut self, same: fn(&V, &V) -> bool) -> Self {
        self.same = same;
        self
    }

    /// Where changes are announced. Set once; a second call is ignored.
    pub fn set_notify(&self, notify: Notify) {
        let _ = self.notify.set(notify);
    }

    /// Tells the windows that the data behind `topic` changed.
    pub fn announce(&self, topic: &str) {
        if let Some(notify) = self.notify.get() {
            notify(topic);
        }
    }

    /// The answer for `key`. `read` builds the future that reads it; it is called at
    /// most once, while a lock is held, so it must only create the future. `topic` is
    /// announced when a refresh changed what a window may be showing. `refresh` starts
    /// a read now and waits for it, whatever is kept.
    pub async fn get<F, Fut>(&self, key: &str, topic: &str, refresh: bool, read: F) -> Result<V>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V>> + Send + 'static,
    {
        let mut flight = match self.begin(key, topic, refresh, read) {
            Begun::Answer(value) => return Ok(value),
            Begun::Wait(flight) => flight,
        };
        let answer = match flight.wait_for(Option::is_some).await {
            Ok(answer) => answer.clone(),
            Err(_) => None,
        };
        match answer.as_deref() {
            Some(Ok(value)) => Ok(value.clone()),
            Some(Err(error)) => Err(error.duplicate()),
            None => Err(CoreError::Internal("a shared read ended early".into())),
        }
    }

    fn begin<F, Fut>(&self, key: &str, topic: &str, refresh: bool, read: F) -> Begun<V>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V>> + Send + 'static,
    {
        let now = Instant::now();
        let mut guard = lock(&self.state);
        let state = &mut *guard;
        if !state.entries.contains_key(key) {
            self.make_room(state);
            state.entries.insert(
                key.to_owned(),
                Entry {
                    value: None,
                    at: now,
                    touched: now,
                    retry_after: None,
                    flight: None,
                    floor: 0,
                },
            );
        }
        let entry = state.entries.get_mut(key).expect("inserted above");
        entry.touched = now;
        let age = now.saturating_duration_since(entry.at);
        if !refresh
            && let Some(value) = &entry.value
            && age < self.max_age
        {
            let value = value.clone();
            let quiet = entry.flight.is_none() && entry.retry_after.is_none_or(|at| now >= at);
            if age >= self.fresh && quiet {
                self.start(state, key, topic, read());
            }
            return Begun::Answer(value);
        }
        if !refresh && let Some((_, flight)) = &entry.flight {
            return Begun::Wait(flight.clone());
        }
        Begun::Wait(self.start(state, key, topic, read()))
    }

    /// Runs `read` on its own task, so a caller that gives up (a closed request) cannot
    /// leave the key stuck, and records its answer unless something newer has landed.
    fn start<Fut>(&self, state: &mut State<V>, key: &str, topic: &str, read: Fut) -> Flight<V>
    where
        Fut: Future<Output = Result<V>> + Send + 'static,
    {
        let id = state.next;
        state.next += 1;
        let (sender, receiver) = watch::channel(None);
        if let Some(entry) = state.entries.get_mut(key) {
            entry.flight = Some((id, receiver.clone()));
            // Whatever an older read still running finds, this one is newer.
            entry.floor = entry.floor.max(id);
        }
        let shared = self.state.clone();
        let notify = self.notify.get().cloned();
        let (keep, same) = (self.keep, self.same);
        let (key, topic) = (key.to_owned(), topic.to_owned());
        tokio::spawn(async move {
            let guard = FlightGuard {
                state: shared.clone(),
                key: key.clone(),
                id,
            };
            let result = read.await;
            let mut changed = false;
            {
                let mut state = lock(&shared);
                if let Some(entry) = state.entries.get_mut(&key) {
                    match &result {
                        Ok(value) if id >= entry.floor && keep(value) => {
                            changed = entry.value.as_ref().is_some_and(|old| !same(old, value));
                            entry.value = Some(value.clone());
                            entry.at = Instant::now();
                            entry.retry_after = None;
                            entry.floor = id + 1;
                        }
                        Ok(value) if !keep(value) && entry.value.is_some() => {
                            entry.retry_after = Some(Instant::now() + RETRY_AFTER_FAILURE);
                        }
                        Err(_) if entry.value.is_some() => {
                            entry.retry_after = Some(Instant::now() + RETRY_AFTER_FAILURE);
                        }
                        _ => {}
                    }
                }
            }
            drop(guard);
            let _ = sender.send(Some(Arc::new(result)));
            if changed && let Some(notify) = notify {
                notify(&topic);
            }
        });
        receiver
    }

    /// Frees the least recently used key when the cap is reached. A key whose read is
    /// running stays, since callers wait on it.
    fn make_room(&self, state: &mut State<V>) {
        while state.entries.len() >= self.cap {
            let oldest = state
                .entries
                .iter()
                .filter(|(_, entry)| entry.flight.is_none())
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(key, _)| key.clone());
            match oldest {
                Some(key) => {
                    state.entries.remove(&key);
                }
                None => return,
            }
        }
    }

    /// Keeps `value` as the answer for `key`, for a write that already knows it.
    pub fn put(&self, key: &str, value: V) {
        let now = Instant::now();
        let mut guard = lock(&self.state);
        let state = &mut *guard;
        if !state.entries.contains_key(key) {
            self.make_room(state);
        }
        let id = state.next;
        state.next += 1;
        let entry = state.entries.entry(key.to_owned()).or_insert(Entry {
            value: None,
            at: now,
            touched: now,
            retry_after: None,
            flight: None,
            floor: 0,
        });
        entry.value = Some(value);
        entry.at = now;
        entry.touched = now;
        entry.retry_after = None;
        entry.flight = None;
        entry.floor = id + 1;
    }

    /// Drops every key starting with `prefix` after a write that changed their data. A
    /// read already running for them will not be kept, so the next call reads again.
    /// The caller announces what changed, once, however many caches it dropped.
    pub fn invalidate(&self, prefix: &str) {
        {
            let mut guard = lock(&self.state);
            let state = &mut *guard;
            let floor = state.next;
            state.next += 1;
            for (_, entry) in state
                .entries
                .iter_mut()
                .filter(|(key, _)| key.starts_with(prefix))
            {
                entry.value = None;
                entry.flight = None;
                entry.retry_after = None;
                entry.floor = floor + 1;
            }
        }
    }

    /// Drops everything kept.
    pub fn invalidate_all(&self) {
        self.invalidate("");
    }

    /// Makes every kept value older by `by`, to test what a stale value does.
    #[cfg(test)]
    pub(crate) fn age_all(&self, by: Duration) {
        for entry in lock(&self.state).entries.values_mut() {
            entry.at = entry.at.checked_sub(by).unwrap_or(entry.at);
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        lock(&self.state).entries.len()
    }
}

#[cfg(test)]
mod tests;
