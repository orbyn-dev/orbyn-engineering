//! The QuickJS executor: `rquickjs` with a memory limit, a stack limit and an interrupt hook.

use crate::{
    CodeExecutor, ErrorKind, Limits, Run, RunError, RunSeed, Step, ToolFailure, ToolHost, strip,
};
use async_trait::async_trait;
use rquickjs::prelude::Async;
use rquickjs::{AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Function, Promise};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
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
        let runtime = AsyncRuntime::new().map_err(engine)?;
        runtime.set_memory_limit(limits.heap_bytes).await;
        runtime.set_max_stack_size(limits.stack_bytes).await;
        let shared = Arc::new(Shared {
            clock: Mutex::new(Clock {
                used: Duration::ZERO,
                since: None,
            }),
            cpu_hit: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            logs: Mutex::new(Logs::default()),
        });
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
        Err(caught) => return Err(classify(&caught.to_string(), shared)),
    };
    let settled = promise.into_future::<rquickjs::Value>().await.catch(ctx);
    shared.pause_clock();
    match settled {
        Ok(_) => {
            let text: String = globals.get("__out").map_err(|e| internal(&e.to_string()))?;
            serde_json::from_str(&text).map_err(|e| internal(&e.to_string()))
        }
        Err(caught) => Err(classify_caught(ctx, &caught, shared)),
    }
}

fn classify_caught(ctx: &Ctx<'_>, caught: &rquickjs::CaughtError<'_>, shared: &Shared) -> RunError {
    let mut error = classify(&caught.to_string(), shared);
    if let rquickjs::CaughtError::Exception(exception) = caught {
        let code: Option<String> = exception.get("code").ok();
        if code.as_deref() == Some("limit") {
            error.kind = ErrorKind::Limit;
        }
        error.code = code;
        error.line = exception.stack().as_deref().and_then(first_line);
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

fn classify(message: &str, shared: &Shared) -> RunError {
    let lower = message.to_lowercase();
    if shared.cpu_hit.load(Ordering::SeqCst) || lower.contains("interrupted") {
        return limit_error("The script used more than its CPU time.".into());
    }
    if lower.contains("out of memory") {
        return limit_error("The script used more than its memory limit (heap).".into());
    }
    if lower.contains("call stack") || lower.contains("stack overflow") {
        return limit_error("The script used more than its stack limit.".into());
    }
    let syntax = lower.contains("syntaxerror") || lower.contains("unexpected token");
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
