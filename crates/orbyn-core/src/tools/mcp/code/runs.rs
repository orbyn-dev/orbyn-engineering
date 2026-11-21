//! The script runs: each is its own task, owned here and not by the tool call that started it, so a
//! call that returns after one slice never drops a run or expires an approval it waits on.

use orbyn_code_sandbox::{CodeExecutor, Limits, RunError, RunSeed, Step, ToolHost};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
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
/// How long a finished run answers a resume.
const KEEP_FINISHED: Duration = Duration::from_secs(10 * 60);
/// 40 s: five seconds under `TOOL_BUDGET`, so the outer call answers before its timer drops it.
pub(super) const SLICE: Duration = Duration::from_secs(40);

pub(super) struct Live {
    thread_id: String,
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
}

pub(crate) struct CodeRuns {
    executor: Mutex<Option<Arc<dyn CodeExecutor>>>,
    runs: Mutex<HashMap<String, Arc<Live>>>,
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
            runs: Mutex::new(HashMap::new()),
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

    fn runs(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<Live>>> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The run `run_id` of `thread_id`. Another thread's run does not exist.
    pub(super) fn get(&self, run_id: &str, thread_id: &str) -> Option<Arc<Live>> {
        self.sweep();
        self.runs()
            .get(run_id)
            .filter(|live| live.thread_id == thread_id)
            .cloned()
    }

    fn sweep(&self) {
        self.runs().retain(|_, live| {
            let at = *live
                .finished_at
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            at.is_none_or(|at| at.elapsed() < KEEP_FINISHED)
        });
    }

    fn is_live(live: &Live) -> bool {
        live.finished_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_none()
    }

    /// Prepares `code` and starts it as its own task. `host` is given the list the run reports
    /// its calls in flight on.
    pub(super) async fn start(
        &self,
        run_id: &str,
        thread_id: &str,
        code: &str,
        seed: RunSeed,
        host: impl FnOnce(PendingList) -> Arc<dyn ToolHost>,
    ) -> Result<Arc<Live>, Refused> {
        self.sweep();
        {
            let runs = self.runs();
            let live: Vec<_> = runs.values().filter(|l| Self::is_live(l)).collect();
            if live.len() >= MAX_LIVE {
                return Err(Refused::Limit(format!(
                    "Orbyn is running {MAX_LIVE} scripts already. Wait for one to finish."
                )));
            }
            if live.iter().filter(|l| l.thread_id == thread_id).count() >= MAX_LIVE_PER_THREAD {
                return Err(Refused::Limit(format!(
                    "This thread has {MAX_LIVE_PER_THREAD} scripts running. Resume one with its runId, or wait."
                )));
            }
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
            *marker.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
            let _ = sender.send(Some(step));
        });
        let live = Arc::new(Live {
            thread_id: thread_id.to_owned(),
            done,
            pending,
            finished_at,
            task,
        });
        self.runs().insert(run_id.to_owned(), live.clone());
        Ok(live)
    }
}
