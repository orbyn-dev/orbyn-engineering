//! The script runs: each is its own task, owned here and not by the tool call that started it, so a
//! call that returns after one slice never drops a run or expires an approval it waits on.

use orbyn_code_sandbox::{CodeExecutor, Limits, RunError, RunSeed, Step, ToolHost};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// A call in flight, as `paused` reports it.
#[derive(Clone, Debug)]
pub(super) struct Pending {
    pub tool: String,
    pub call: usize,
}

pub(super) type PendingList = Arc<Mutex<Vec<Pending>>>;

const MAX_LIVE_PER_THREAD: usize = 4;
const MAX_LIVE: usize = 16;
/// Crew worker threads together, so workers cannot take the lead's place in the core.
const MAX_LIVE_WORKERS: usize = 8;
/// How long a finished run answers a resume.
const KEEP_FINISHED: Duration = Duration::from_secs(10 * 60);
/// 40 s: five seconds under `TOOL_BUDGET`, so the outer call answers before its timer drops it.
pub(super) const SLICE: Duration = Duration::from_secs(40);

pub(super) struct Live {
    thread_id: String,
    crew_worker: bool,
    done: watch::Receiver<Option<Step>>,
    pending: PendingList,
    finished_at: Arc<Mutex<Option<Instant>>>,
    task: JoinHandle<()>,
}

impl Drop for Live {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Live {
    pub(super) fn pending(&self) -> Vec<Pending> {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The run's end, or `None` when `slice` passes first.
    pub(super) async fn wait(&self, slice: Duration) -> Option<Step> {
        let mut done = self.done.clone();
        match tokio::time::timeout(slice, done.wait_for(Option::is_some)).await {
            Ok(Ok(step)) => step.clone(),
            _ => None,
        }
    }
}

pub(super) enum Refused {
    /// The start was refused before any script ran.
    Limit(String),
    /// The code did not parse or is over a limit.
    Script(RunError),
    /// The run id is already live or starting.
    Duplicate(String),
}

/// A run id held from the cap check until its run is inserted. `prepare` awaits in between, so
/// without this a burst of starts would all pass the caps and then all insert.
#[derive(Default)]
struct Slots {
    runs: HashMap<String, Arc<Live>>,
    starting: HashMap<String, Starting>,
}

struct Starting {
    thread_id: String,
    crew_worker: bool,
}

impl Slots {
    fn sweep(&mut self) {
        self.runs.retain(|_, live| {
            let at = *live
                .finished_at
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            at.is_none_or(|at| at.elapsed() < KEEP_FINISHED)
        });
    }
}

/// Releases a reserved start slot if the start does not reach the map, on an early return, an
/// error or a panic.
struct Reservation<'a> {
    runs: &'a CodeRuns,
    run_id: String,
    held: bool,
}

impl Reservation<'_> {
    fn commit(mut self) {
        self.held = false;
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if self.held {
            self.runs.lock().starting.remove(&self.run_id);
        }
    }
}

pub(crate) struct CodeRuns {
    executor: Mutex<Option<Arc<dyn CodeExecutor>>>,
    slots: Mutex<Slots>,
    slice: Mutex<Duration>,
}

#[cfg(feature = "code-mode")]
fn default_executor() -> Option<Arc<dyn CodeExecutor>> {
    Some(Arc::new(orbyn_code_sandbox::QuickJsExecutor))
}

#[cfg(not(feature = "code-mode"))]
fn default_executor() -> Option<Arc<dyn CodeExecutor>> {
    None
}

impl CodeRuns {
    pub(crate) fn new() -> Self {
        Self {
            executor: Mutex::new(default_executor()),
            slots: Mutex::new(Slots::default()),
            slice: Mutex::new(SLICE),
        }
    }

    /// Replaces the executor (tests play scripts with a fake one).
    #[cfg(test)]
    pub(crate) fn set_executor(&self, executor: Arc<dyn CodeExecutor>, slice: Duration) {
        *self.executor.lock().unwrap_or_else(PoisonError::into_inner) = Some(executor);
        *self.slice.lock().unwrap_or_else(PoisonError::into_inner) = slice;
    }

    pub(super) fn slice(&self) -> Duration {
        *self.slice.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock(&self) -> MutexGuard<'_, Slots> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The run `run_id` of `thread_id`. Another thread's run does not exist.
    pub(super) fn get(&self, run_id: &str, thread_id: &str) -> Option<Arc<Live>> {
        let mut slots = self.lock();
        slots.sweep();
        slots
            .runs
            .get(run_id)
            .filter(|live| live.thread_id == thread_id)
            .cloned()
    }

    fn is_live(live: &Live) -> bool {
        live.finished_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
    }

    /// Forgets every live run, as a restart would (tests).
    #[cfg(test)]
    pub(crate) fn forget_all(&self) {
        self.lock().runs.clear();
    }

    /// Whether `run_id` is a live run of any thread.
    pub(super) fn is_running(&self, run_id: &str) -> bool {
        let mut slots = self.lock();
        slots.sweep();
        slots.runs.get(run_id).is_some_and(|l| Self::is_live(l))
    }

    /// Prepares `code` and starts it as its own task. `host` is given the list the run reports
    /// its calls in flight on; `finish` sees the end first and may change it (a replay that could not
    /// go on ends as lost) before it is announced.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn start(
        &self,
        run_id: &str,
        thread_id: &str,
        crew_worker: bool,
        code: &str,
        seed: RunSeed,
        host: impl FnOnce(PendingList) -> Arc<dyn ToolHost>,
        finish: impl FnOnce(Step) -> Step + Send + 'static,
    ) -> Result<Arc<Live>, Refused> {
        let mut reservation = Reservation {
            runs: self,
            run_id: run_id.to_owned(),
            held: false,
        };
        {
            let mut slots = self.lock();
            slots.sweep();
            // A finished run kept for a resume is replaced by a replay of the same id; only a live
            // or starting run is a duplicate.
            if slots.starting.contains_key(run_id)
                || slots
                    .runs
                    .get(run_id)
                    .is_some_and(|live| Self::is_live(live))
            {
                return Err(Refused::Duplicate(format!(
                    "The run {run_id} is already running."
                )));
            }
            let live = slots
                .runs
                .values()
                .filter(|live| Self::is_live(live))
                .count();
            if live + slots.starting.len() >= MAX_LIVE {
                return Err(Refused::Limit(format!(
                    "Orbyn is running {MAX_LIVE} scripts already. Wait for one to finish."
                )));
            }
            let thread_live = slots
                .runs
                .values()
                .filter(|live| Self::is_live(live) && live.thread_id == thread_id)
                .count();
            let thread_starting = slots
                .starting
                .values()
                .filter(|start| start.thread_id == thread_id)
                .count();
            if thread_live + thread_starting >= MAX_LIVE_PER_THREAD {
                return Err(Refused::Limit(format!(
                    "This thread has {MAX_LIVE_PER_THREAD} scripts running. Resume one with its runId, or wait."
                )));
            }
            let worker_live = slots
                .runs
                .values()
                .filter(|live| Self::is_live(live) && live.crew_worker)
                .count();
            let worker_starting = slots
                .starting
                .values()
                .filter(|start| start.crew_worker)
                .count();
            if crew_worker && worker_live + worker_starting >= MAX_LIVE_WORKERS {
                return Err(Refused::Limit(
                    "Too many Crew worker runs are live. Wait for one to finish.".to_owned(),
                ));
            }
            slots.starting.insert(
                run_id.to_owned(),
                Starting {
                    thread_id: thread_id.to_owned(),
                    crew_worker,
                },
            );
            reservation.held = true;
        }
        let executor = self
            .executor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(executor) = executor else {
            return Err(Refused::Limit(
                "This build of Orbyn cannot run scripts.".into(),
            ));
        };
        let limits = Limits::default();
        let wall = limits.wall;
        let pending: PendingList = Arc::default();
        let run = executor
            .prepare(code, limits, seed, host(pending.clone()))
            .await
            .map_err(Refused::Script)?;
        let (sender, done) = watch::channel(None);
        let finished_at = Arc::new(Mutex::new(None));
        let marker = finished_at.clone();
        let task = tokio::spawn(async move {
            let mut run = run;
            let step = run
                .drive(Instant::now() + wall + Duration::from_secs(5))
                .await;
            let step = finish(step);
            *marker.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
            let _ = sender.send(Some(step));
        });
        let live = Arc::new(Live {
            thread_id: thread_id.to_owned(),
            crew_worker,
            done,
            pending,
            finished_at,
            task,
        });
        {
            let mut slots = self.lock();
            slots.starting.remove(run_id);
            slots.runs.insert(run_id.to_owned(), live.clone());
        }
        reservation.commit();
        Ok(live)
    }
}
