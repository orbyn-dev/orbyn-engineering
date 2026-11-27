//! The platform-independent capture flow: one id, `started`, then `ready`, `excluded` or `failed`, within ten
//! seconds and cancelled by its id. A second start is ignored.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::exclusion;
use super::pending::{self, Capture, Pending};
use super::settings::Settings;

pub const MAX_PNG_BYTES: usize = 40 * 1024 * 1024;
const CAPTURE_LIMIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Permission,
    NoWindow,
    CaptureError,
    Excluded { app_name: String },
}

pub struct Captured {
    pub png: Vec<u8>,
    pub app_name: String,
    pub window_title: String,
    pub app_id: String,
    pub accessible_text: Option<String>,
}

pub trait Capturer: Send + Sync {
    fn capture_frontmost(
        &self,
        is_excluded: &dyn Fn(&str, &str) -> bool,
    ) -> Result<Captured, Failure>;

    /// Asks the capturer to give up and return soon. The default does nothing: the stub and the
    /// Windows capturer have no long platform read to stop. The macOS capturer checks it before
    /// the AX read and the AX walk checks it on every node, so a capture the flow has given up on
    /// cannot keep reading another app's window.
    fn cancel(&self) {}
}

pub trait Events: Send + Sync {
    fn emit(&self, name: &str, payload: Value);
}

type Active = (String, Arc<AtomicBool>);

#[derive(Default)]
pub struct Gate {
    active: Mutex<Option<Active>>,
}

impl Gate {
    pub fn cancel(&self, id: &str) -> bool {
        let Ok(mut active) = self.active.lock() else {
            return false;
        };
        let cancelled = match active.as_ref() {
            Some((current, cancelled)) if current == id => Some(cancelled.clone()),
            _ => None,
        };
        let Some(cancelled) = cancelled else {
            return false;
        };
        cancelled.store(true, Ordering::SeqCst);
        // Release the slot now, so a fresh start does not reuse the discarded id.
        *active = None;
        true
    }

    fn settle(&self, id: &str, action: impl FnOnce()) {
        let proceed = {
            let Ok(mut active) = self.active.lock() else {
                return;
            };
            let cancelled = match active.as_ref() {
                Some((current, cancelled)) if current == id => Some(cancelled.clone()),
                _ => None,
            };
            let Some(cancelled) = cancelled else {
                return;
            };
            let proceed = !cancelled.load(Ordering::SeqCst);
            *active = None;
            proceed
        };
        if proceed {
            action();
        }
    }
}

#[derive(Clone)]
pub struct Flow {
    pub(crate) pending: Arc<Mutex<Pending>>,
    pub(crate) gate: Arc<Gate>,
    events: Arc<dyn Events>,
    limit: Duration,
}

impl Flow {
    pub fn new(pending: Arc<Mutex<Pending>>, gate: Arc<Gate>, events: Arc<dyn Events>) -> Self {
        Self {
            pending,
            gate,
            events,
            limit: CAPTURE_LIMIT,
        }
    }

    pub fn begin(&self) -> Result<(String, Option<Arc<AtomicBool>>), ()> {
        let (id, cancelled) = {
            let mut active = self.gate.active.lock().map_err(|_| ())?;
            if let Some((current, _)) = active.as_ref() {
                return Ok((current.clone(), None));
            }
            let id = pending::new_id();
            let cancelled = Arc::new(AtomicBool::new(false));
            *active = Some((id.clone(), cancelled.clone()));
            (id, cancelled)
        };
        self.events.emit("snapshot://started", json!({ "id": id }));
        Ok((id, Some(cancelled)))
    }

    pub fn run(
        &self,
        id: String,
        cancelled: Arc<AtomicBool>,
        capturer: Box<dyn Capturer>,
        settings: Settings,
        own_bundle_id: String,
    ) {
        // One shared handle: the thread reads through it and the flow asks it to stop when the
        // capture is discarded or times out.
        let capturer: Arc<dyn Capturer> = Arc::from(capturer);
        if cancelled.load(Ordering::SeqCst) {
            capturer.cancel();
            self.gate.settle(&id, || {});
            return;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let reading = Arc::clone(&capturer);
        let _ = std::thread::spawn(move || {
            let excluded = |app_id: &str, app_name: &str| {
                exclusion::is_excluded(app_id, app_name, &own_bundle_id, &settings)
            };
            let _ = sender.send(reading.capture_frontmost(&excluded));
        });
        let deadline = Instant::now() + self.limit;
        loop {
            if cancelled.load(Ordering::SeqCst) {
                // The page discarded the capture: stop the read instead of leaving it running.
                capturer.cancel();
                self.gate.settle(&id, || {});
                return;
            }
            let now = Instant::now();
            if now >= deadline {
                // Ask the capture thread to stop, so a platform read that honours the flag cannot
                // outlive the ten-second capture limit.
                capturer.cancel();
                self.fail(&id, "timeout");
                return;
            }
            match receiver.recv_timeout((deadline - now).min(Duration::from_millis(10))) {
                Ok(Ok(captured)) => {
                    let stored = id.clone();
                    self.gate.settle(&id, move || self.store(&stored, captured));
                    return;
                }
                Ok(Err(Failure::Excluded { app_name })) => {
                    let event_id = id.clone();
                    self.gate.settle(&id, move || {
                        self.events.emit(
                            "snapshot://excluded",
                            json!({ "id": event_id, "app_name": app_name }),
                        )
                    });
                    return;
                }
                Ok(Err(failure)) => {
                    let reason = match failure {
                        Failure::Permission => "permission",
                        Failure::NoWindow => "no_window",
                        Failure::CaptureError => "capture_error",
                        Failure::Excluded { .. } => "excluded",
                    };
                    self.fail(&id, reason);
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.fail(&id, "capture_error");
                    return;
                }
            }
        }
    }

    fn fail(&self, id: &str, reason: &str) {
        self.gate.settle(id, || {
            self.events
                .emit("snapshot://failed", json!({ "id": id, "reason": reason }))
        });
    }

    fn store(&self, id: &str, captured: Captured) {
        if captured.png.len() > MAX_PNG_BYTES {
            self.events.emit(
                "snapshot://failed",
                json!({ "id": id, "reason": "too_large" }),
            );
            return;
        }
        let capture = Capture::new(
            captured.png,
            captured.app_name,
            captured.window_title,
            captured.app_id,
            pending::captured_at_now(),
        )
        .with_accessible_text(captured.accessible_text);
        let stored = match self.pending.lock() {
            Ok(mut pending) => {
                pending.insert_with_id(id.to_owned(), capture);
                true
            }
            Err(_) => false,
        };
        if stored {
            self.events.emit("snapshot://ready", json!({ "id": id }));
        } else {
            self.events.emit(
                "snapshot://failed",
                json!({ "id": id, "reason": "capture_error" }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: &str = "com.example.own";
    type Seen = Arc<Mutex<Vec<(String, Value)>>>;

    struct FakeEvents(Seen);

    impl Events for FakeEvents {
        fn emit(&self, name: &str, payload: Value) {
            self.0.lock().unwrap().push((name.to_owned(), payload));
        }
    }

    struct Fake(fn() -> Result<Captured, Failure>, Duration);

    impl Capturer for Fake {
        fn capture_frontmost(
            &self,
            _is_excluded: &dyn Fn(&str, &str) -> bool,
        ) -> Result<Captured, Failure> {
            std::thread::sleep(self.1);
            (self.0)()
        }
    }

    fn ready() -> Result<Captured, Failure> {
        Ok(Captured {
            png: b"png bytes".to_vec(),
            app_name: "Notes".to_owned(),
            window_title: "Weekly plan".to_owned(),
            app_id: "com.apple.Notes".to_owned(),
            accessible_text: None,
        })
    }

    fn ready_with_text() -> Result<Captured, Failure> {
        Ok(Captured {
            png: b"png bytes".to_vec(),
            app_name: "Notes".to_owned(),
            window_title: "Weekly plan".to_owned(),
            app_id: "com.apple.Notes".to_owned(),
            accessible_text: Some("Line one\nLine two".to_owned()),
        })
    }

    fn excluded() -> Result<Captured, Failure> {
        Err(Failure::Excluded {
            app_name: "1Password".to_owned(),
        })
    }

    fn oversized() -> Result<Captured, Failure> {
        Ok(Captured {
            png: vec![0; MAX_PNG_BYTES + 1],
            app_name: String::new(),
            window_title: String::new(),
            app_id: String::new(),
            accessible_text: None,
        })
    }

    fn failure(kind: u8) -> Result<Captured, Failure> {
        Err(match kind {
            0 => Failure::Permission,
            1 => Failure::NoWindow,
            _ => Failure::CaptureError,
        })
    }

    fn set_up(limit: Duration) -> (Seen, Flow) {
        let seen: Seen = Arc::new(Mutex::new(Vec::new()));
        let mut flow = Flow::new(
            Arc::new(Mutex::new(Pending::default())),
            Arc::new(Gate::default()),
            Arc::new(FakeEvents(seen.clone())),
        );
        flow.limit = limit;
        (seen, flow)
    }

    fn names(seen: &Seen) -> String {
        seen.lock()
            .unwrap()
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    fn start(flow: &Flow, outcome: fn() -> Result<Captured, Failure>, sleep: Duration) -> String {
        let (id, cancelled) = flow.begin().unwrap();
        flow.run(
            id.clone(),
            cancelled.unwrap(),
            Box::new(Fake(outcome, sleep)),
            Settings {
                snapshot_enabled: true,
                ..Settings::default()
            },
            OWN.to_owned(),
        );
        id
    }

    fn wait_for_id(seen: &Seen) -> String {
        for _ in 0..250 {
            if let Some((_, payload)) = seen.lock().unwrap().first() {
                return payload["id"].as_str().unwrap().to_owned();
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("no started event");
    }

    #[test]
    fn ready_is_stored_once_and_excluded_keeps_nothing() {
        let (seen, flow) = set_up(Duration::from_secs(1));
        let id = start(&flow, ready, Duration::ZERO);
        assert_eq!(names(&seen), "snapshot://started,snapshot://ready");
        let mut pending = flow.pending.lock().unwrap();
        let capture = pending.take(&id).unwrap();
        assert_eq!(capture.png(), b"png bytes");
        assert!(pending.take(&id).is_none());

        let (seen, flow) = set_up(Duration::from_secs(1));
        let id = start(&flow, excluded, Duration::ZERO);
        assert_eq!(names(&seen), "snapshot://started,snapshot://excluded");
        assert_eq!(
            seen.lock().unwrap()[1].1,
            json!({ "id": id, "app_name": "1Password" })
        );
        assert!(flow.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn accessible_text_reaches_the_pending_capture() {
        let (seen, flow) = set_up(Duration::from_secs(1));
        let id = start(&flow, ready_with_text, Duration::ZERO);
        assert_eq!(names(&seen), "snapshot://started,snapshot://ready");
        let mut pending = flow.pending.lock().unwrap();
        let capture = pending.take(&id).unwrap();
        assert_eq!(capture.accessible_text(), Some("Line one\nLine two"));
    }

    #[test]
    fn every_failure_reports_its_reason() {
        let cases: [(fn() -> Result<Captured, Failure>, &str); 4] = [
            (|| failure(0), "permission"),
            (|| failure(1), "no_window"),
            (oversized, "too_large"),
            (|| failure(2), "capture_error"),
        ];
        for (outcome, reason) in cases {
            let (seen, flow) = set_up(Duration::from_secs(1));
            let _ = start(&flow, outcome, Duration::ZERO);
            assert_eq!(names(&seen), "snapshot://started,snapshot://failed");
            assert_eq!(seen.lock().unwrap()[1].1["reason"], reason);
            assert!(flow.pending.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn a_discard_during_the_capture_emits_nothing_more() {
        let (seen, flow) = set_up(Duration::from_secs(1));
        let running = {
            let flow = flow.clone();
            std::thread::spawn(move || start(&flow, ready, Duration::from_millis(500)))
        };
        let id = wait_for_id(&seen);
        assert!(flow.gate.cancel(&id));
        running.join().unwrap();
        assert_eq!(names(&seen), "snapshot://started");
        assert!(flow.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn a_capture_can_start_right_after_a_discard() {
        let (seen, flow) = set_up(Duration::from_secs(1));
        let running = {
            let flow = flow.clone();
            std::thread::spawn(move || start(&flow, ready, Duration::from_millis(500)))
        };
        let first = wait_for_id(&seen);
        assert!(flow.gate.cancel(&first));

        let (second, cancelled) = flow.begin().unwrap();
        assert_ne!(second, first);
        let cancelled = cancelled.expect("a discard releases the slot");
        flow.run(
            second.clone(),
            cancelled,
            Box::new(Fake(ready, Duration::ZERO)),
            Settings {
                snapshot_enabled: true,
                ..Settings::default()
            },
            OWN.to_owned(),
        );
        running.join().unwrap();

        assert_eq!(
            names(&seen),
            "snapshot://started,snapshot://started,snapshot://ready"
        );
        let mut pending = flow.pending.lock().unwrap();
        assert!(pending.take(&second).is_some());
        assert!(pending.take(&first).is_none());
    }

    #[test]
    fn a_second_start_is_ignored() {
        let (seen, flow) = set_up(Duration::from_secs(1));
        let running = {
            let flow = flow.clone();
            std::thread::spawn(move || start(&flow, ready, Duration::from_millis(500)))
        };
        let id = wait_for_id(&seen);
        let (second, cancelled) = flow.begin().unwrap();
        assert_eq!(second, id);
        assert!(cancelled.is_none());
        running.join().unwrap();
        assert_eq!(names(&seen), "snapshot://started,snapshot://ready");
        assert_eq!(flow.pending.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_slow_capture_times_out() {
        let (seen, flow) = set_up(Duration::from_millis(20));
        let _ = start(&flow, ready, Duration::from_millis(200));
        assert_eq!(names(&seen), "snapshot://started,snapshot://failed");
        assert_eq!(seen.lock().unwrap()[1].1["reason"], "timeout");
        assert!(flow.pending.lock().unwrap().is_empty());
    }

    /// A capturer that keeps reading until either its own five seconds run out or the flow cancels
    /// it, and records that it ended. Like the macOS capturer, it only stops when asked.
    struct Stoppable {
        stop: Arc<AtomicBool>,
        ended: Arc<AtomicBool>,
    }

    impl Capturer for Stoppable {
        fn capture_frontmost(
            &self,
            _is_excluded: &dyn Fn(&str, &str) -> bool,
        ) -> Result<Captured, Failure> {
            let started = Instant::now();
            while !self.stop.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(2));
            }
            self.ended.store(true, Ordering::SeqCst);
            (ready)()
        }

        fn cancel(&self) {
            self.stop.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn a_capture_over_the_limit_is_asked_to_stop() {
        let (seen, flow) = set_up(Duration::from_millis(20));
        let stop = Arc::new(AtomicBool::new(false));
        let ended = Arc::new(AtomicBool::new(false));
        let (id, cancelled) = flow.begin().unwrap();
        flow.run(
            id,
            cancelled.unwrap(),
            Box::new(Stoppable {
                stop: Arc::clone(&stop),
                ended: Arc::clone(&ended),
            }),
            Settings {
                snapshot_enabled: true,
                ..Settings::default()
            },
            OWN.to_owned(),
        );
        assert_eq!(names(&seen), "snapshot://started,snapshot://failed");
        assert_eq!(seen.lock().unwrap()[1].1["reason"], "timeout");
        assert!(stop.load(Ordering::SeqCst), "the flow asked it to stop");
        for _ in 0..500 {
            if ended.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(ended.load(Ordering::SeqCst), "the capture thread ended");
        assert!(flow.pending.lock().unwrap().is_empty());
    }
}
