//! The QuickJS executor: `rquickjs` with a memory limit, a stack limit and an interrupt hook.

use crate::{
    CodeExecutor, ErrorKind, Limits, Run, RunError, RunSeed, Step, ToolFailure, ToolHost, strip,
};
use async_trait::async_trait;
use rquickjs::allocator::{Allocator, RustAllocator};
use rquickjs::prelude::Async;
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Function, Promise};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

const PRELUDE: &str = include_str!("prelude.js");

pub struct QuickJsExecutor;

/// Script CPU time: the time JavaScript has been running, not the time spent waiting on a tool.
struct Clock {
    used: Duration,
    since: Option<Instant>,
}

#[derive(Default)]
struct Logs {
    lines: VecDeque<String>,
    bytes: usize,
    truncated: bool,
}

struct Shared {
    clock: Mutex<Clock>,
    cpu_hit: AtomicBool,
    heap_hit: AtomicBool,
    calls: AtomicUsize,
    logs: Mutex<Logs>,
}

impl Shared {
    fn pause_clock(&self) {
        let mut clock = self.clock.lock().expect("clock");
        if let Some(since) = clock.since.take() {
            clock.used += since.elapsed();
        }
    }

    fn resume_clock(&self) {
        self.clock.lock().expect("clock").since = Some(Instant::now());
    }

    fn log(&self, text: String, limits: &Limits) {
        let mut logs = self.logs.lock().expect("logs");
        logs.bytes += text.len();
        logs.lines.push_back(text);
        while logs.lines.len() > limits.log_lines || logs.bytes > limits.log_bytes {
            let Some(old) = logs.lines.pop_front() else {
                break;
            };
            logs.bytes -= old.len();
            logs.truncated = true;
        }
    }

    fn take_logs(&self) -> (Vec<String>, bool) {
        let logs = self.logs.lock().expect("logs");
        (logs.lines.iter().cloned().collect(), logs.truncated)
    }
}

type Job = Pin<Box<dyn Future<Output = Step> + Send>>;

/// The sandbox heap: every byte the engine asks for is counted against its limit, and a request
/// the limit refuses is recorded in `heap_hit`.
///
/// The record is needed because the engine cannot always say what happened. With the heap full it
/// cannot allocate the error that says "out of memory" either, and throws `null` instead (see
/// `JS_ThrowError2` in `quickjs.c`), so the text `classify` used to match is not there. Counting
/// the bytes ourselves makes the refusal an observed fact.
struct Heap {
    used: usize,
    limit: usize,
    shared: Arc<Shared>,
}

impl Heap {
    /// A limit of 0 is no limit, which is how [`Limits::heap_bytes`] reads.
    fn new(limit: usize, shared: Arc<Shared>) -> Self {
        Self {
            used: 0,
            limit: if limit == 0 { usize::MAX } else { limit },
            shared,
        }
    }

    /// Whether `add` bytes fit once `free` bytes are given back, and the record of a refusal.
    fn fits(&mut self, free: usize, add: usize) -> bool {
        let held = self.used.saturating_sub(free);
        let total = held.saturating_add(add);
        if total > self.limit {
            self.shared.heap_hit.store(true, Ordering::SeqCst);
            return false;
        }
        true
    }
}

// SAFETY: every method hands QuickJS a block from `RustAllocator`, which returns null or memory
// aligned to a `u64` of at least the size asked for, so the trait's contract holds. `usable_size`
// is the size of the block that same allocator handed out.
unsafe impl Allocator for Heap {
    fn alloc(&mut self, size: usize) -> *mut u8 {
        if !self.fits(0, size) {
            return ptr::null_mut();
        }
        let pointer = RustAllocator.alloc(size);
        if !pointer.is_null() {
            self.used = self
                .used
                .saturating_add(unsafe { RustAllocator::usable_size(pointer) });
        }
        pointer
    }

    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
        let Some(bytes) = count.checked_mul(size) else {
            return ptr::null_mut();
        };
        if !self.fits(0, bytes) {
            return ptr::null_mut();
        }
        let pointer = RustAllocator.calloc(count, size);
        if !pointer.is_null() {
            self.used = self
                .used
                .saturating_add(unsafe { RustAllocator::usable_size(pointer) });
        }
        pointer
    }

    unsafe fn dealloc(&mut self, pointer: *mut u8) {
        let size = unsafe { RustAllocator::usable_size(pointer) };
        self.used = self.used.saturating_sub(size);
        unsafe { RustAllocator.dealloc(pointer) };
    }

    unsafe fn realloc(&mut self, pointer: *mut u8, new_size: usize) -> *mut u8 {
        if pointer.is_null() {
            return self.alloc(new_size);
        }
        let free = unsafe { RustAllocator::usable_size(pointer) };
        if !self.fits(free, new_size) {
            return ptr::null_mut();
        }
        let moved = unsafe { RustAllocator.realloc(pointer, new_size) };
        if !moved.is_null() {
            self.used = self
                .used
                .saturating_sub(free)
                .saturating_add(unsafe { RustAllocator::usable_size(moved) });
        }
        moved
    }

    unsafe fn usable_size(pointer: *mut u8) -> usize {
        if pointer.is_null() {
            0
        } else {
            unsafe { RustAllocator::usable_size(pointer) }
        }
    }
}

struct QuickJsRun {
    job: Job,
    finished: Option<Step>,
    wall_deadline: Instant,
    shared: Arc<Shared>,
}

#[async_trait]
impl CodeExecutor for QuickJsExecutor {
    async fn prepare(
        &self,
        source: &str,
        limits: Limits,
        seed: RunSeed,
        host: Arc<dyn ToolHost>,
    ) -> Result<Box<dyn Run>, RunError> {
        if source.len() > limits.source_bytes {
            return Err(limit_error(format!(
                "The script is over {} bytes.",
                limits.source_bytes
            )));
        }
        let js = strip::to_js(source)?;
        let engine = |error: rquickjs::Error| RunError {
            kind: ErrorKind::Runtime,
            message: format!("The sandbox could not start: {error}"),
            line: None,
            code: None,
        };
        let shared = Arc::new(Shared {
            clock: Mutex::new(Clock {
                used: Duration::ZERO,
                since: None,
            }),
            cpu_hit: AtomicBool::new(false),
            heap_hit: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            logs: Mutex::new(Logs::default()),
        });
        let runtime = AsyncRuntime::new_with_alloc(Heap::new(limits.heap_bytes, shared.clone()))
            .map_err(engine)?;
        runtime.set_max_stack_size(limits.stack_bytes).await;
        let watched = shared.clone();
        let cpu = limits.cpu;
        runtime
            .set_interrupt_handler(Some(Box::new(move || {
                let clock = watched.clock.lock().expect("clock");
                let running = clock.since.map_or(Duration::ZERO, |since| since.elapsed());
                let over = clock.used + running > cpu;
                if over {
                    watched.cpu_hit.store(true, Ordering::SeqCst);
                }
                over
            })))
            .await;
        let context = AsyncContext::full(&runtime).await.map_err(engine)?;
        let wall_deadline = Instant::now() + limits.wall;
        let job_shared = shared.clone();
        let job: Job = Box::pin(async move {
            // The runtime lives as long as the job.
            let _runtime = runtime;
            run_script(context, js, limits, seed, host, job_shared).await
        });
        Ok(Box::new(QuickJsRun {
            job,
            finished: None,
            wall_deadline,
            shared,
        }))
    }
}

#[async_trait]
impl Run for QuickJsRun {
    async fn drive(&mut self, until: Instant) -> Step {
        if let Some(step) = &self.finished {
            return step.clone();
        }
        let wall_over = self.wall_deadline <= until;
        let stop = tokio::time::Instant::from_std(until.min(self.wall_deadline));
        match tokio::time::timeout_at(stop, &mut self.job).await {
            Ok(step) => {
                self.finished = Some(step.clone());
                step
            }
            Err(_) if wall_over => {
                let (logs, truncated) = self.shared.take_logs();
                let step = Step::Failed {
                    error: limit_error("The run took longer than its time limit.".into()),
                    logs,
                    truncated,
                };
                self.finished = Some(step.clone());
                step
            }
            Err(_) => Step::Yielded,
        }
    }
}

fn limit_error(message: String) -> RunError {
    RunError {
        kind: ErrorKind::Limit,
        message,
        line: None,
        code: None,
    }
}

async fn run_script(
    context: AsyncContext,
    js: String,
    limits: Limits,
    seed: RunSeed,
    host: Arc<dyn ToolHost>,
    shared: Arc<Shared>,
) -> Step {
    let outcome = context
        .async_with({
            let shared = shared.clone();
            let limits = limits.clone();
            async move |ctx| execute(&ctx, js, &limits, seed, host, &shared).await
        })
        .await;
    let (logs, truncated) = shared.take_logs();
    match outcome {
        Ok(result) => Step::Done {
            result,
            logs,
            truncated,
        },
        Err(error) => Step::Failed {
            error,
            logs,
            truncated,
        },
    }
}

async fn execute<'js>(
    ctx: &Ctx<'js>,
    js: String,
    limits: &Limits,
    seed: RunSeed,
    host: Arc<dyn ToolHost>,
    shared: &Arc<Shared>,
) -> Result<Value, RunError> {
    let internal = |what: &str| RunError {
        kind: ErrorKind::Runtime,
        message: format!("The sandbox failed: {what}"),
        line: None,
        code: None,
    };
    let globals = ctx.globals();
    let permits = Arc::new(Semaphore::new(limits.in_flight));
    let call_shared = shared.clone();
    let max_calls = limits.calls;
    let call = Function::new(
        ctx.clone(),
        Async(move |name: String, args: String| {
            let host = host.clone();
            let shared = call_shared.clone();
            let permits = permits.clone();
            async move { host_call(host, shared, permits, max_calls, name, args).await }
        }),
    )
    .map_err(|e| internal(&e.to_string()))?;
    let log_shared = shared.clone();
    let log_limits = limits.clone();
    let log = Function::new(ctx.clone(), move |level: String, text: String| {
        let prefix = if level == "log" {
            String::new()
        } else {
            format!("[{level}] ")
        };
        log_shared.log(format!("{prefix}{text}"), &log_limits);
    })
    .map_err(|e| internal(&e.to_string()))?;
    let bad = |e: rquickjs::Error| internal(&e.to_string());
    globals.set("__call", call).map_err(bad)?;
    globals.set("__log", log).map_err(bad)?;
    globals.set("__start", seed.start_ms as f64).map_err(bad)?;
    globals.set("__seed", f64::from(seed.seed)).map_err(bad)?;
    ctx.eval::<(), _>(PRELUDE)
        .map_err(|e| internal(&e.to_string()))?;

    shared.resume_clock();
    let source = format!(
        "{js}\n;const __v = await __orbyn_main();\nglobalThis.__out = JSON.stringify(__v === undefined ? null : __v);"
    );
    let promise: Promise = match ctx.eval_promise(source).catch(ctx) {
        Ok(promise) => promise,
        Err(caught) => return Err(classify(&caught, shared)),
    };
    let settled = promise.into_future::<rquickjs::Value>().await.catch(ctx);
    shared.pause_clock();
    match settled {
        Ok(_) => {
            let text: String = globals.get("__out").map_err(|e| internal(&e.to_string()))?;
            serde_json::from_str(&text).map_err(|e| internal(&e.to_string()))
        }
        Err(caught) => {
            // Reading the exception's properties runs the script's own getters, so the clock stays
            // running while it is inspected: the work a getter hides is still the script's CPU, and
            // the interrupt hook ends it, with `classify` reading that `cpu_hit` before anything else.
            shared.resume_clock();
            let error = classify_caught(ctx, &caught, shared);
            shared.pause_clock();
            Err(error)
        }
    }
}

fn classify_caught(ctx: &Ctx<'_>, caught: &rquickjs::CaughtError<'_>, shared: &Shared) -> RunError {
    let mut error = classify(caught, shared);
    if let rquickjs::CaughtError::Exception(exception) = caught {
        let code: Option<String> = exception.get("code").ok();
        if code.as_deref() == Some("limit") {
            error.kind = ErrorKind::Limit;
        }
        error.code = code;
        error.line = exception.stack().as_deref().and_then(first_line);
    }
    // Those reads run the script's own getters, so the CPU limit can be reached after `classify`
    // looked: the engine's record decides, not the order the reads happened in.
    if shared.cpu_hit.load(Ordering::SeqCst) {
        return limit_error("The script used more than its CPU time.".into());
    }
    let _ = ctx;
    error
}

/// The first `:line:column` in a stack trace.
fn first_line(stack: &str) -> Option<u32> {
    stack.lines().find_map(|line| {
        let mut parts = line.trim_end_matches(')').rsplit(':');
        let _column = parts.next()?.parse::<u32>().ok()?;
        parts.next()?.parse::<u32>().ok()
    })
}

/// The thrown exception's class and message, as read once. Both properties are the script's own getters, so a second
/// read can answer something else: what is shown and what is classified have to come from the same read.
struct Thrown {
    name: Option<String>,
    message: Option<String>,
}

/// The engine's own error class and message, when the run ended with a JavaScript `Error`.
fn thrown(caught: &rquickjs::CaughtError<'_>) -> Option<Thrown> {
    let rquickjs::CaughtError::Exception(exception) = caught else {
        return None;
    };
    Some(Thrown {
        name: exception.get::<_, Option<String>>("name").ok().flatten(),
        message: exception.message(),
    })
}

/// Whether what ended the run says nothing about why: `null`, `undefined`, a blank string, an `Error` with an empty
/// message, or an allocation the Rust side could not make. Anything else, including a thrown number or object, is
/// what the script threw and is reported as it is.
fn carries_no_reason(caught: &rquickjs::CaughtError<'_>, thrown: Option<&Thrown>) -> bool {
    match caught {
        // The message alone decides: a `name` that is absent or cannot be read does not make a thrown `Error`
        // reasonless, and replacing its message with a limit's would be a lie.
        rquickjs::CaughtError::Exception(_) => thrown
            .and_then(|thrown| thrown.message.as_ref())
            .is_none_or(|text| text.trim().is_empty()),
        rquickjs::CaughtError::Value(value) => {
            if value.is_null() || value.is_undefined() {
                return true;
            }
            value
                .as_string()
                .is_some_and(|text| text.to_string().map_or(true, |text| text.trim().is_empty()))
        }
        rquickjs::CaughtError::Error(error) => matches!(error, rquickjs::Error::Allocation),
    }
}

fn classify(caught: &rquickjs::CaughtError<'_>, shared: &Shared) -> RunError {
    let thrown = thrown(caught);
    // The words a person reads are built from that one read as well, so a getter that answers something else the
    // second time cannot make the message disagree with the class it was given. Only a value that is not a
    // JavaScript `Error` reaches the engine's formatting, which is the one path with no getter to read.
    let message = match &thrown {
        Some(Thrown { name, message }) => match (name, message) {
            (Some(name), Some(message)) => format!("{name}: {message}"),
            (Some(name), None) => name.clone(),
            (None, Some(message)) => message.clone(),
            (None, None) => String::new(),
        },
        None => caught.to_string(),
    };
    // The engine records why it stopped the script, so a limit is read from that record and never
    // from the words in the error: `cpu_hit` from the interrupt hook, `heap_hit` from the heap.
    if shared.cpu_hit.load(Ordering::SeqCst) {
        return limit_error("The script used more than its CPU time.".into());
    }
    // `InternalError` is the class the engine uses for its own resources, and is the only thing
    // that says "out of memory"; a script throwing an `Error` with those words stays a runtime
    // error. A heap that refuses an allocation and explains nothing is the heap limit as well.
    let engine_says = |class: &str, text: &str| match &thrown {
        Some(Thrown {
            name: Some(name),
            message: Some(message),
        }) => name.as_str() == class && message.as_str() == text,
        _ => false,
    };
    if engine_says("InternalError", "out of memory")
        || (shared.heap_hit.load(Ordering::SeqCst) && carries_no_reason(caught, thrown.as_ref()))
    {
        return limit_error("The script used more than its memory limit (heap).".into());
    }
    if engine_says("RangeError", "Maximum call stack size exceeded") {
        return limit_error("The script used more than its stack limit.".into());
    }
    let syntax = thrown.as_ref().and_then(|thrown| thrown.name.as_deref()) == Some("SyntaxError");
    RunError {
        kind: if syntax {
            ErrorKind::Syntax
        } else {
            ErrorKind::Runtime
        },
        message: message.trim().to_owned(),
        line: None,
        code: None,
    }
}

async fn host_call(
    host: Arc<dyn ToolHost>,
    shared: Arc<Shared>,
    permits: Arc<Semaphore>,
    max_calls: usize,
    name: String,
    args: String,
) -> String {
    let failure =
        |code: &str, message: String| json!({ "err": { "code": code, "message": message } });
    if shared.calls.fetch_add(1, Ordering::SeqCst) >= max_calls {
        return failure("limit", format!("A script may make {max_calls} calls.")).to_string();
    }
    let Ok(args) = serde_json::from_str::<Value>(&args) else {
        return failure("invalid", "The arguments are not JSON.".into()).to_string();
    };
    shared.pause_clock();
    let permit = permits.acquire().await;
    let reply = host.call(&name, args).await;
    drop(permit);
    shared.resume_clock();
    match reply {
        Ok(value) => json!({ "ok": value }).to_string(),
        Err(ToolFailure { code, message }) => failure(&code, message).to_string(),
    }
}
