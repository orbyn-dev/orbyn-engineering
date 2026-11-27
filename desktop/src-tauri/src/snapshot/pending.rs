//! Pending Snap Shots: in memory only, at most five, each for five minutes.

use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::accessible::MAX_ACCESSIBLE_TEXT_CHARS;

pub const MAX_PENDING: usize = 5;
pub const PENDING_TTL: Duration = Duration::from_secs(5 * 60);
pub const MAX_APP_NAME_CHARS: usize = 255;
pub const MAX_WINDOW_TITLE_CHARS: usize = 1_000;
pub const MAX_APP_ID_CHARS: usize = 255;

static FALLBACK_ID: AtomicU64 = AtomicU64::new(1);

/// Cuts at a character boundary, never inside one, to the page's UTF-16 length limit.
fn clipped(value: String, max_units: usize) -> String {
    let mut used = 0;
    for (index, character) in value.char_indices() {
        used += character.len_utf16();
        if used > max_units {
            return value[..index].to_owned();
        }
    }
    value
}

/// Cuts at a character boundary, never inside one, to a character count.
fn clipped_chars(mut value: String, max_chars: usize) -> String {
    if let Some((index, _)) = value.char_indices().nth(max_chars) {
        value.truncate(index);
    }
    value
}

pub(crate) type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

pub struct Capture {
    png: Vec<u8>,
    app_name: String,
    window_title: String,
    app_id: String,
    captured_at: String,
    accessible_text: Option<String>,
}

impl Capture {
    pub fn new(
        png: Vec<u8>,
        app_name: String,
        window_title: String,
        app_id: String,
        captured_at: String,
    ) -> Self {
        Self {
            png,
            app_name: clipped(app_name, MAX_APP_NAME_CHARS),
            window_title: clipped(window_title, MAX_WINDOW_TITLE_CHARS),
            app_id: clipped(app_id, MAX_APP_ID_CHARS),
            captured_at,
            accessible_text: None,
        }
    }

    pub fn with_accessible_text(mut self, text: Option<String>) -> Self {
        self.accessible_text = text.and_then(|text| {
            let text = clipped_chars(text, MAX_ACCESSIBLE_TEXT_CHARS);
            (!text.trim().is_empty()).then_some(text)
        });
        self
    }

    pub fn accessible_text(&self) -> Option<&str> {
        self.accessible_text.as_deref()
    }

    pub fn png(&self) -> &[u8] {
        &self.png
    }

    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    pub fn window_title(&self) -> &str {
        &self.window_title
    }

    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    pub fn captured_at(&self) -> &str {
        &self.captured_at
    }
}

impl fmt::Debug for Capture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Capture")
            .field("png_bytes", &self.png.len())
            .field("app_name_bytes", &self.app_name.len())
            .field("window_title_bytes", &self.window_title.len())
            .field("app_id_bytes", &self.app_id.len())
            .field("captured_at_bytes", &self.captured_at.len())
            .field(
                "accessible_text_bytes",
                &self.accessible_text.as_ref().map_or(0, |text| text.len()),
            )
            .finish()
    }
}

struct Entry {
    id: String,
    capture: Capture,
    stored_at: Instant,
}

/// Pending captures with an injectable clock, so expiry tests do not sleep.
pub struct Pending {
    entries: VecDeque<Entry>,
    clock: Clock,
}

impl Default for Pending {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            clock: Arc::new(Instant::now),
        }
    }
}

impl Pending {
    /// A pending set whose time comes from `clock`; the tests use this.
    #[cfg(test)]
    pub(crate) fn with_clock(clock: Clock) -> Self {
        Self {
            entries: VecDeque::new(),
            clock,
        }
    }

    fn now(&self) -> Instant {
        (self.clock)()
    }

    #[cfg(test)]
    pub(crate) fn insert(&mut self, capture: Capture) -> String {
        let id = new_id();
        self.insert_with_id(id.clone(), capture);
        id
    }

    pub(crate) fn insert_with_id(&mut self, id: String, capture: Capture) {
        let now = self.now();
        self.insert_at(id, capture, now);
    }

    fn insert_at(&mut self, id: String, capture: Capture, now: Instant) {
        self.sweep_at(now);
        if let Some(index) = self.entries.iter().position(|entry| entry.id == id) {
            self.entries.remove(index);
        }
        self.entries.push_back(Entry {
            id,
            capture,
            stored_at: now,
        });
        while self.entries.len() > MAX_PENDING {
            self.entries.pop_front();
        }
    }

    pub fn take(&mut self, id: &str) -> Option<Capture> {
        let now = self.now();
        self.sweep_at(now);
        let index = self.entries.iter().position(|entry| entry.id == id)?;
        self.entries.remove(index).map(|entry| entry.capture)
    }

    pub fn discard(&mut self, id: &str) {
        self.entries.retain(|entry| entry.id != id);
    }

    pub fn sweep(&mut self) {
        let now = self.now();
        self.sweep_at(now);
    }

    fn sweep_at(&mut self, now: Instant) {
        self.entries
            .retain(|entry| now.saturating_duration_since(entry.stored_at) < PENDING_TTL);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl fmt::Debug for Pending {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Pending")
            .field("captures", &self.entries.len())
            .finish()
    }
}

pub(crate) fn new_id() -> String {
    let mut bytes = [0_u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        let counter = FALLBACK_ID.fetch_add(1, Ordering::Relaxed);
        bytes[..8].copy_from_slice(&counter.to_le_bytes());
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let nanos = u64::try_from(nanos).unwrap_or(u64::MAX);
        bytes[8..].copy_from_slice(&nanos.to_le_bytes());
    }
    hex(&bytes)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let byte = *byte;
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    text
}

pub fn captured_at_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    format_captured_at(seconds)
}

fn format_captured_at(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let remainder = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = remainder / 3_600;
    let minute = (remainder % 3_600) / 60;
    let second = remainder % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (
        year + i64::from(month <= 2),
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::Mutex;

    fn capture(png: &[u8], app_name: &str) -> Capture {
        Capture::new(
            png.to_vec(),
            app_name.to_owned(),
            "Weekly plan".to_owned(),
            "com.example.notes".to_owned(),
            "2026-11-22T10:00:00Z".to_owned(),
        )
    }

    fn clocked(start: Instant) -> (Arc<Mutex<Instant>>, Pending) {
        let clock = Arc::new(Mutex::new(start));
        let handle = Arc::clone(&clock);
        let pending = Pending::with_clock(Arc::new(move || *handle.lock().expect("test clock")));
        (clock, pending)
    }

    fn advance(clock: &Arc<Mutex<Instant>>, by: Duration) {
        *clock.lock().expect("test clock") += by;
    }

    #[test]
    fn ids_are_sixteen_random_bytes_as_lowercase_hex() {
        let (_, mut pending) = clocked(Instant::now());
        let ids: Vec<String> = (0..8)
            .map(|index| pending.insert(capture(b"png", &index.to_string())))
            .collect();
        assert!(ids.iter().all(|id| id.len() == 32));
        assert!(ids.iter().all(|id| {
            id.chars()
                .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
        }));
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
        assert_eq!(pending.len(), MAX_PENDING);
    }

    #[test]
    fn the_sixth_capture_drops_the_oldest() {
        let (_, mut pending) = clocked(Instant::now());
        let ids: Vec<String> = (0..6)
            .map(|index| pending.insert(capture(b"png", &index.to_string())))
            .collect();
        assert!(pending.take(&ids[0]).is_none());
        for id in &ids[1..] {
            assert!(pending.take(id).is_some());
        }
        assert!(pending.is_empty());
    }

    #[test]
    fn take_answers_once_and_discard_removes() {
        let (_, mut pending) = clocked(Instant::now());
        let id = pending.insert(capture(b"png", "Notes"));
        assert!(pending.take(&id).is_some());
        assert!(pending.take(&id).is_none());

        let id = pending.insert(capture(b"png", "Notes"));
        pending.discard(&id);
        assert!(pending.take(&id).is_none());
    }

    #[test]
    fn captures_expire_after_five_minutes() {
        let start = Instant::now();
        let (clock, mut pending) = clocked(start);
        let live = pending.insert(capture(b"png", "Notes"));
        let stale = pending.insert(capture(b"png", "Notes"));

        advance(&clock, PENDING_TTL - Duration::from_secs(1));
        assert!(pending.take(&live).is_some());

        advance(&clock, Duration::from_secs(1));
        assert!(pending.take(&stale).is_none());

        let fresh = pending.insert(capture(b"png", "Notes"));
        assert!(pending.take(&fresh).is_some());
        let fresh = pending.insert(capture(b"png", "Notes"));
        advance(&clock, PENDING_TTL);
        assert!(pending.take(&fresh).is_none());
        assert!(pending.is_empty());
    }

    #[test]
    fn source_text_is_cut_to_the_wire_limits_by_character() {
        let capture = Capture::new(
            b"png".to_vec(),
            "a".repeat(MAX_APP_NAME_CHARS + 5),
            "t".repeat(5_000),
            "i".repeat(MAX_APP_ID_CHARS + 5),
            "2026-11-22T10:00:00Z".to_owned(),
        );
        assert_eq!(capture.app_name().chars().count(), MAX_APP_NAME_CHARS);
        assert_eq!(
            capture.window_title().chars().count(),
            MAX_WINDOW_TITLE_CHARS
        );
        assert_eq!(capture.app_id().chars().count(), MAX_APP_ID_CHARS);
        assert!(
            capture
                .window_title()
                .chars()
                .all(|character| character == 't')
        );

        let multibyte = Capture::new(
            b"png".to_vec(),
            String::new(),
            "é".repeat(MAX_WINDOW_TITLE_CHARS + 10),
            String::new(),
            "2026-11-22T10:00:00Z".to_owned(),
        );
        assert_eq!(
            multibyte.window_title().chars().count(),
            MAX_WINDOW_TITLE_CHARS
        );
        assert_eq!(multibyte.window_title().chars().last(), Some('é'));

        let astral = Capture::new(
            b"png".to_vec(),
            String::new(),
            "🙂".repeat(MAX_WINDOW_TITLE_CHARS + 10),
            String::new(),
            "2026-11-22T10:00:00Z".to_owned(),
        );
        assert_eq!(
            astral.window_title().encode_utf16().count(),
            MAX_WINDOW_TITLE_CHARS
        );
        assert_eq!(astral.window_title().chars().last(), Some('🙂'));
    }

    #[test]
    fn accessible_text_is_clipped_by_character_and_blank_text_is_dropped() {
        let text = "é".repeat(MAX_ACCESSIBLE_TEXT_CHARS + 10);
        let item = capture(b"png", "Notes").with_accessible_text(Some(text));
        let stored = item.accessible_text().expect("text is kept");
        assert_eq!(stored.chars().count(), MAX_ACCESSIBLE_TEXT_CHARS);
        assert!(stored.chars().all(|character| character == 'é'));

        let blank = capture(b"png", "Notes").with_accessible_text(Some(" \n\t ".to_owned()));
        assert!(blank.accessible_text().is_none());
        assert!(capture(b"png", "Notes").accessible_text().is_none());
    }

    #[test]
    fn debug_prints_sizes_only() {
        let mut png = vec![b'X'; 64];
        png[0] = 0xff;
        let item = Capture::new(
            png,
            "Private app".to_owned(),
            "Private title".to_owned(),
            "com.private.app".to_owned(),
            "2026-11-22T10:00:00Z".to_owned(),
        )
        .with_accessible_text(Some("Private accessible text".to_owned()));
        let debug = format!("{item:?}");
        assert!(debug.contains("png_bytes"));
        assert!(debug.contains("accessible_text_bytes"));
        assert!(!debug.contains("Private app"));
        assert!(!debug.contains("Private title"));
        assert!(!debug.contains("Private accessible text"));
        assert!(!debug.contains("XXXX"));
        assert!(!format!("{:?}", Pending::default()).contains("Private"));
    }

    #[test]
    fn captured_at_is_iso_utc() {
        assert_eq!(format_captured_at(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_captured_at(1_700_000_000), "2023-11-14T22:13:20Z");
        assert!(captured_at_now().ends_with('Z'));
    }
}
