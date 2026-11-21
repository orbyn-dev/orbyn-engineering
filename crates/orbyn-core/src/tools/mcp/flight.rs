//! Bounds the MCP App tool-info read: a small global cap on how many short engine
//! sessions it starts at once, and single-flight per (thread, render) so concurrent
//! identical reads share one session. Single-flight is not a cache: the answer is handed
//! to the callers that were waiting and nothing is kept once the flight ends, so a later
//! read asks the engine again.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared, WeakShared};
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::error::{CoreError, Result};

/// A flight is one render of one thread.
pub(super) type FlightKey = (String, String);

/// How long a read waits for one of the few slots before it gives up as busy.
pub(super) const QUEUE_BUDGET: Duration = Duration::from_secs(45);
/// Engine sessions the tool-info read may have open at once, service-wide.
const MAX_CONCURRENT_READS: usize = 2;

/// The engine read every app card's tool-info goes through.
pub(super) static TOOL_INFO_READS: LazyLock<FlightGate<Option<Value>>> =
    LazyLock::new(|| FlightGate::new(MAX_CONCURRENT_READS));

type Outcome<T> = std::result::Result<T, CoreError>;
type Flight<T> = Shared<BoxFuture<'static, Arc<Outcome<T>>>>;
type Flights<T> =
    Arc<Mutex<HashMap<FlightKey, (u64, WeakShared<BoxFuture<'static, Arc<Outcome<T>>>>)>>>;

/// Removes its flight from the table when the flight ends, whether it finished or every
/// waiter left and the work was dropped. It removes only its own entry, so a newer flight
/// for the same key is never lost.
struct Ended<T: Clone> {
    flights: Flights<T>,
    key: FlightKey,
    id: u64,
}

impl<T: Clone> Drop for Ended<T> {
    fn drop(&mut self) {
        let mut table = self.flights.lock().unwrap_or_else(PoisonError::into_inner);
        if table.get(&self.key).is_some_and(|(id, _)| *id == self.id) {
            table.remove(&self.key);
        }
    }
}

/// A slot cap plus single-flight keyed by (thread, render).
pub(super) struct FlightGate<T: Clone> {
    slots: Arc<Semaphore>,
    flights: Flights<T>,
    next: AtomicU64,
}

impl<T: Clone + Send + Sync + 'static> FlightGate<T> {
    pub(super) fn new(slots: usize) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(slots)),
            flights: Arc::default(),
            next: AtomicU64::new(0),
        }
    }

    /// Flights in progress; none is kept once its work ended.
    #[cfg(test)]
    pub(super) fn in_flight(&self) -> usize {
        self.flights
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Runs `work` for `key`, or joins the identical read already running. The work
    /// holds one of the few slots while it runs and is dropped when its last waiter
    /// leaves, which ends whatever process it started. Every waiter of one flight gets
    /// the same answer; a later read starts a new flight.
    pub(super) async fn run(
        &self,
        key: FlightKey,
        work: impl Future<Output = Result<T>> + Send + 'static,
    ) -> Result<T> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let slots = self.slots.clone();
        let ended = Ended {
            flights: self.flights.clone(),
            key: key.clone(),
            id,
        };
        let fresh: Flight<T> = async move {
            // Held for the flight's life: it leaves the table however it ends.
            let _ended = ended;
            let permit = tokio::time::timeout(QUEUE_BUDGET, slots.acquire_owned()).await;
            let Ok(Ok(_permit)) = permit else {
                return Arc::new(Err(CoreError::EngineUnavailable(
                    "The app's server is busy with other reads. Try again shortly.".into(),
                )));
            };
            Arc::new(work.await)
        }
        .boxed()
        .shared();
        let joined = {
            let mut table = self.flights.lock().unwrap_or_else(PoisonError::into_inner);
            // The upgraded handle leaves this block, so a flight that ends the instant
            // this one joins is never dropped (and its guard never run) under the lock.
            let running = table.get(&key).and_then(|(_, weak)| weak.upgrade());
            if running.is_none()
                && let Some(weak) = fresh.downgrade()
            {
                table.insert(key, (id, weak));
            }
            running
        };
        let outcome = joined.unwrap_or(fresh).await;
        match outcome.as_ref() {
            Ok(value) => Ok(value.clone()),
            Err(error) => Err(error.duplicate()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;

    fn key(render: &str) -> FlightKey {
        ("thread".to_owned(), render.to_owned())
    }

    /// Counts how many times the work ran and how many ran at once.
    #[derive(Default)]
    struct Probe {
        runs: AtomicUsize,
        active: AtomicUsize,
        most: AtomicUsize,
        dropped: AtomicUsize,
    }

    struct OnDrop(Arc<Probe>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.dropped.fetch_add(1, Ordering::SeqCst);
        }
    }

    async fn work(probe: Arc<Probe>, ms: u64) -> Result<usize> {
        let _watch = OnDrop(probe.clone());
        let run = probe.runs.fetch_add(1, Ordering::SeqCst) + 1;
        let now = probe.active.fetch_add(1, Ordering::SeqCst) + 1;
        probe.most.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(ms)).await;
        probe.active.fetch_sub(1, Ordering::SeqCst);
        Ok(run)
    }

    #[tokio::test(start_paused = true)]
    async fn mcp_app_identical_concurrent_reads_run_once_and_keep_nothing() {
        let gate = FlightGate::<usize>::new(2);
        let probe = Arc::new(Probe::default());
        let all = futures_util::future::join_all(
            (0..8).map(|_| gate.run(key("r1"), work(probe.clone(), 50))),
        )
        .await;
        assert_eq!(
            probe.runs.load(Ordering::SeqCst),
            1,
            "one run for eight reads"
        );
        for reply in all {
            assert_eq!(reply.unwrap(), 1, "every waiter gets that one answer");
        }
        // Not a cache: the flight is over, so the next read runs the work again.
        assert_eq!(
            gate.run(key("r1"), work(probe.clone(), 50)).await.unwrap(),
            2
        );
        assert_eq!(probe.runs.load(Ordering::SeqCst), 2);
        assert_eq!(gate.in_flight(), 0, "nothing is kept after the flights end");
    }

    #[tokio::test(start_paused = true)]
    async fn mcp_app_different_renders_do_not_share_a_flight() {
        let gate = FlightGate::<usize>::new(8);
        let probe = Arc::new(Probe::default());
        let _ = futures_util::future::join_all(
            ["a", "b", "c"].map(|render| gate.run(key(render), work(probe.clone(), 50))),
        )
        .await;
        assert_eq!(probe.runs.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn mcp_app_the_cap_holds_across_distinct_reads() {
        let gate = FlightGate::<usize>::new(2);
        let probe = Arc::new(Probe::default());
        let renders = ["a", "b", "c", "d", "e", "f"];
        let all = futures_util::future::join_all(
            renders.map(|render| gate.run(key(render), work(probe.clone(), 50))),
        )
        .await;
        assert!(
            all.iter().all(Result::is_ok),
            "the queued reads still finish"
        );
        assert_eq!(probe.runs.load(Ordering::SeqCst), 6);
        assert_eq!(
            probe.most.load(Ordering::SeqCst),
            2,
            "never more than the cap at once, and the cap is used"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn mcp_app_a_read_that_waits_too_long_for_a_slot_is_unavailable() {
        let gate = FlightGate::<usize>::new(1);
        let probe = Arc::new(Probe::default());
        let slow = gate.run(key("a"), work(probe.clone(), 1_000_000));
        let queued = gate.run(key("b"), work(probe.clone(), 1));
        let (_, queued) = tokio::join!(
            async {
                // Dropped by timeout so the test ends; the queued read gives up on its own.
                let _ = tokio::time::timeout(QUEUE_BUDGET + Duration::from_secs(5), slow).await;
            },
            queued
        );
        let error = queued.unwrap_err();
        assert!(
            matches!(error, CoreError::EngineUnavailable(_)),
            "{error:?}"
        );
        assert_eq!(
            probe.runs.load(Ordering::SeqCst),
            1,
            "the queued work never ran"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn mcp_app_one_waiter_leaving_keeps_the_flight_for_the_others() {
        let gate = FlightGate::<usize>::new(2);
        let probe = Arc::new(Probe::default());
        let leaves = tokio::time::timeout(
            Duration::from_millis(10),
            gate.run(key("r1"), work(probe.clone(), 100)),
        );
        let stays = gate.run(key("r1"), work(probe.clone(), 100));
        let (left, stayed) = tokio::join!(leaves, stays);
        assert!(left.is_err(), "the first waiter gave up");
        assert_eq!(
            stayed.unwrap(),
            1,
            "the other still got the one run's answer"
        );
        assert_eq!(probe.runs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn mcp_app_the_last_waiter_leaving_cancels_the_work() {
        let gate = FlightGate::<usize>::new(2);
        let probe = Arc::new(Probe::default());
        let left = tokio::time::timeout(
            Duration::from_millis(10),
            gate.run(key("r1"), work(probe.clone(), 100)),
        )
        .await;
        assert!(left.is_err());
        assert_eq!(
            probe.dropped.load(Ordering::SeqCst),
            1,
            "the work was dropped, which is what kills the engine process"
        );
        assert_eq!(gate.in_flight(), 0);
        assert_eq!(probe.active.load(Ordering::SeqCst), 1, "it never finished");
        // The slot is free again and a new read starts a new run.
        assert_eq!(
            gate.run(key("r1"), work(probe.clone(), 1)).await.unwrap(),
            2
        );
    }

    #[tokio::test(start_paused = true)]
    async fn mcp_app_a_failed_flight_hands_every_waiter_the_same_failure() {
        let gate = FlightGate::<usize>::new(2);
        let fail = || async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            Err::<usize, _>(CoreError::EngineUnavailable("down".into()))
        };
        let all = futures_util::future::join_all((0..3).map(|_| gate.run(key("r1"), fail()))).await;
        for reply in all {
            assert!(matches!(reply, Err(CoreError::EngineUnavailable(text)) if text == "down"));
        }
        assert_eq!(gate.in_flight(), 0);
    }
}
