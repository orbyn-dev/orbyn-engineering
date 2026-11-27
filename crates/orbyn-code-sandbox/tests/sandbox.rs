use async_trait::async_trait;
use orbyn_code_sandbox::{
    CodeExecutor, ErrorKind, Limits, QuickJsExecutor, Run, RunSeed, Step, ToolFailure, ToolHost,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// `echo` returns its args, `slow` waits `ms` first, `deny` fails with code `denied`.
struct Host {
    calls: AtomicUsize,
}

#[async_trait]
impl ToolHost for Host {
    async fn call(&self, name: &str, args: Value) -> Result<Value, ToolFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match name {
            "echo" => Ok(args),
            "slow" => {
                tokio::time::sleep(Duration::from_millis(args["ms"].as_u64().unwrap_or(0))).await;
                Ok(json!("slept"))
            }
            "deny" => Err(ToolFailure {
                code: "denied".into(),
                message: "The person said no.".into(),
            }),
            other => Err(ToolFailure {
                code: "not_found".into(),
                message: format!("no {other}"),
            }),
        }
    }
}

fn seed() -> RunSeed {
    RunSeed {
        start_ms: 1_700_000_000_000,
        seed: 7,
    }
}

async fn start(code: &str, limits: Limits) -> (Box<dyn Run>, Arc<Host>) {
    let host = Arc::new(Host {
        calls: AtomicUsize::new(0),
    });
    let run = QuickJsExecutor
        .prepare(code, limits, seed(), host.clone())
        .await
        .expect("prepare");
    (run, host)
}

async fn finish(code: &str, limits: Limits) -> Step {
    let (mut run, _) = start(code, limits).await;
    run.drive(Instant::now() + Duration::from_secs(60)).await
}

/// The class a run ends with and the message a person would read, whether `prepare` refused the
/// source or the run failed.
async fn failure(code: &str, limits: Limits) -> (ErrorKind, String) {
    let host = Arc::new(Host {
        calls: AtomicUsize::new(0),
    });
    let error = match QuickJsExecutor.prepare(code, limits, seed(), host).await {
        Err(error) => error,
        Ok(mut run) => failed(run.drive(Instant::now() + Duration::from_secs(60)).await),
    };
    (error.kind, error.message)
}

fn done(step: Step) -> Value {
    match step {
        Step::Done { result, .. } => result,
        other => panic!("expected done, got {other:?}"),
    }
}

fn failed(step: Step) -> orbyn_code_sandbox::RunError {
    match step {
        Step::Failed { error, .. } => error,
        other => panic!("expected failure, got {other:?}"),
    }
}

#[tokio::test]
async fn returns_a_value_and_undefined_is_null() {
    assert_eq!(
        done(finish("return { a: [1, 2] }", Limits::default()).await),
        json!({ "a": [1, 2] })
    );
    assert_eq!(
        done(finish("const x = 1;", Limits::default()).await),
        Value::Null
    );
}

#[tokio::test]
async fn strips_typescript_types() {
    let code = "interface P { n: number }\nconst p: P = { n: 2 };\nreturn (p.n as number) + 1;";
    assert_eq!(done(finish(code, Limits::default()).await), json!(3));
}

#[tokio::test]
async fn calls_a_tool_and_runs_calls_together() {
    let code = "const a = await tools.echo({ x: 1 });\n\
        const all = await Promise.all([1, 2, 3].map((n) => tools.echo({ n })));\nreturn [a, all];";
    let (mut run, host) = start(code, Limits::default()).await;
    let step = run.drive(Instant::now() + Duration::from_secs(30)).await;
    assert_eq!(
        done(step),
        json!([{ "x": 1 }, [{ "n": 1 }, { "n": 2 }, { "n": 3 }]])
    );
    assert_eq!(host.calls.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn a_syntax_error_names_its_line() {
    let (_, error) = match QuickJsExecutor
        .prepare(
            "const a = 1;\nconst = ;",
            Limits::default(),
            seed(),
            Arc::new(Host { calls: 0.into() }),
        )
        .await
    {
        Ok(_) => (
            0,
            failed(finish("const a = 1;\nconst = ;", Limits::default()).await),
        ),
        Err(error) => (0, error),
    };
    assert_eq!(error.kind, ErrorKind::Syntax);
}

#[tokio::test]
async fn a_thrown_error_is_a_runtime_error_with_its_line() {
    let error = failed(finish("const a = 1;\nthrow new Error('boom');", Limits::default()).await);
    assert_eq!(error.kind, ErrorKind::Runtime);
    assert!(error.message.contains("boom"), "{}", error.message);
    assert_eq!(error.line, Some(2));
}

#[tokio::test]
async fn a_refusal_throws_and_a_script_may_catch_it() {
    let caught = "try { await tools.deny({}); } catch (e) { return [e.code, e.message]; }";
    assert_eq!(
        done(finish(caught, Limits::default()).await),
        json!(["denied", "The person said no."])
    );
    let error = failed(finish("await tools.deny({}); return 1;", Limits::default()).await);
    assert_eq!(error.kind, ErrorKind::Runtime);
    assert_eq!(error.code.as_deref(), Some("denied"));
}

#[tokio::test]
async fn an_infinite_loop_ends_as_a_cpu_limit() {
    let limits = Limits {
        cpu: Duration::from_millis(200),
        ..Limits::default()
    };
    let error = failed(finish("while (true) {}", limits).await);
    assert_eq!(error.kind, ErrorKind::Limit);
    assert!(error.message.contains("CPU"), "{}", error.message);
}

#[tokio::test]
async fn waiting_on_a_tool_does_not_use_cpu_time() {
    let limits = Limits {
        cpu: Duration::from_millis(150),
        ..Limits::default()
    };
    let step = finish("return await tools.slow({ ms: 500 });", limits).await;
    assert_eq!(done(step), json!("slept"));
}

#[tokio::test]
async fn too_much_memory_ends_as_a_heap_limit() {
    let limits = Limits {
        heap_bytes: 4 * 1024 * 1024,
        ..Limits::default()
    };
    let error = failed(
        finish(
            "const a = []; while (true) a.push('x'.repeat(1000));",
            limits,
        )
        .await,
    );
    assert_eq!(error.kind, ErrorKind::Limit);
    assert!(error.message.contains("heap"), "{}", error.message);
}

#[tokio::test]
async fn deep_recursion_ends_as_a_stack_limit() {
    let error = failed(finish("const f = () => f() + 1; return f();", Limits::default()).await);
    assert_eq!(error.kind, ErrorKind::Limit);
}

#[tokio::test]
async fn the_call_count_is_limited() {
    let limits = Limits {
        calls: 3,
        ..Limits::default()
    };
    let code = "for (let i = 0; i < 5; i++) await tools.echo({ i }); return 1;";
    let (mut run, host) = start(code, limits).await;
    let error = failed(run.drive(Instant::now() + Duration::from_secs(30)).await);
    assert_eq!(error.kind, ErrorKind::Limit);
    assert_eq!(host.calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn a_source_over_the_limit_is_refused() {
    let limits = Limits {
        source_bytes: 10,
        ..Limits::default()
    };
    let host = Arc::new(Host { calls: 0.into() });
    let refused = QuickJsExecutor
        .prepare("return 1; // long enough", limits, seed(), host)
        .await;
    assert_eq!(refused.err().map(|e| e.kind), Some(ErrorKind::Limit));
}

#[tokio::test]
async fn nothing_reaches_outside() {
    let probe = "return [typeof fetch, typeof XMLHttpRequest, typeof process, typeof require, \
        typeof Deno, typeof std, typeof os, typeof WebSocket, typeof setTimeout, typeof globalThis.__call];";
    let undefined = json!([
        "undefined",
        "undefined",
        "undefined",
        "undefined",
        "undefined",
        "undefined",
        "undefined",
        "undefined",
        "undefined",
        "undefined"
    ]);
    assert_eq!(done(finish(probe, Limits::default()).await), undefined);
}

#[tokio::test]
async fn code_cannot_be_generated_from_text() {
    for attempt in [
        "return eval('1 + 1');",
        "return new Function('return 1')();",
        "return (function () {}).constructor('return 1')();",
        "return (async function () {}).constructor('return 1')();",
        "return Object.getPrototypeOf(async () => {}).constructor('return 1')();",
    ] {
        let error = failed(finish(attempt, Limits::default()).await);
        assert_eq!(error.kind, ErrorKind::Runtime, "{attempt}");
        assert!(
            error.message.contains("not allowed"),
            "{attempt}: {}",
            error.message
        );
    }
}

#[tokio::test]
async fn the_clock_and_random_repeat() {
    let code = "return [Date.now(), new Date().getTime(), Math.random(), Math.random()];";
    let first = done(finish(code, Limits::default()).await);
    let second = done(finish(code, Limits::default()).await);
    assert_eq!(first, second);
    assert_eq!(first[0], json!(1_700_000_000_000_i64));
    assert_eq!(first[0], first[1]);
    assert_ne!(first[2], first[3]);
}

#[tokio::test]
async fn logs_are_kept_and_the_oldest_drop_over_the_limit() {
    let limits = Limits {
        log_lines: 3,
        ..Limits::default()
    };
    let step = finish(
        "for (let i = 0; i < 5; i++) console.log('line', i); console.error({ a: 1 });",
        limits,
    )
    .await;
    let Step::Done {
        logs, truncated, ..
    } = step
    else {
        panic!("not done")
    };
    assert!(truncated);
    assert_eq!(
        logs,
        vec![
            "line 3".to_string(),
            "line 4".to_string(),
            "[error] {\"a\":1}".to_string()
        ]
    );
}

#[tokio::test]
async fn a_run_yields_while_a_call_is_pending_and_resumes() {
    let (mut run, _) = start("return await tools.slow({ ms: 400 });", Limits::default()).await;
    let first = run.drive(Instant::now() + Duration::from_millis(80)).await;
    assert!(matches!(first, Step::Yielded), "{first:?}");
    let second = run.drive(Instant::now() + Duration::from_secs(30)).await;
    assert_eq!(done(second), json!("slept"));
    // A finished run answers the same again.
    assert_eq!(done(run.drive(Instant::now()).await), json!("slept"));
}

#[tokio::test]
async fn the_wall_clock_ends_a_run() {
    let limits = Limits {
        wall: Duration::from_millis(100),
        ..Limits::default()
    };
    let (mut run, _) = start("await tools.slow({ ms: 5000 }); return 1;", limits).await;
    let error = failed(run.drive(Instant::now() + Duration::from_secs(30)).await);
    assert_eq!(error.kind, ErrorKind::Limit);
}

#[tokio::test]
async fn each_failure_lands_in_its_own_class() {
    let heap = Limits {
        heap_bytes: 4 * 1024 * 1024,
        ..Limits::default()
    };
    let cpu = Limits {
        cpu: Duration::from_millis(200),
        ..Limits::default()
    };
    let wall = Limits {
        wall: Duration::from_millis(100),
        ..Limits::default()
    };
    // The first row is the one that was classified as a runtime error: a heap full of live data
    // leaves QuickJS unable to build even the error that says "out of memory", so it throws
    // `null` and the text to match is gone. Every other row must keep its own class.
    let cases = [
        (
            "heap limit, grown in a loop",
            "const a = []; while (true) a.push('x'.repeat(1000));",
            heap.clone(),
            ErrorKind::Limit,
            "heap",
        ),
        (
            "heap limit, refused in one allocation",
            "const a = 'x'.repeat(64 * 1024 * 1024); return a.length;",
            heap.clone(),
            ErrorKind::Limit,
            "heap",
        ),
        (
            "own throw after a heap refusal",
            "try { 'x'.repeat(64 * 1024 * 1024); } catch (e) {} throw new Error('recovered');",
            heap.clone(),
            ErrorKind::Runtime,
            "recovered",
        ),
        (
            "syntax error",
            "const a = 1;\nconst = ;",
            Limits::default(),
            ErrorKind::Syntax,
            "syntax",
        ),
        ("cpu limit", "while (true) {}", cpu, ErrorKind::Limit, "cpu"),
        (
            "wall clock",
            "await tools.slow({ ms: 5000 }); return 1;",
            wall,
            ErrorKind::Limit,
            "time limit",
        ),
        (
            "ordinary throw",
            "throw new Error('boom');",
            Limits::default(),
            ErrorKind::Runtime,
            "boom",
        ),
        (
            "null throw",
            "throw null;",
            Limits::default(),
            ErrorKind::Runtime,
            "",
        ),
    ];
    for (name, code, limits, kind, message) in cases {
        let (got, text) = failure(code, limits).await;
        assert_eq!(got, kind, "{name}: {text}");
        assert!(text.to_lowercase().contains(message), "{name}: {text}");
    }
}

#[tokio::test]
async fn a_heap_under_its_limit_is_fine() {
    // The heap is counted by the executor, so a run that stays under the limit must still hold
    // what it allocated: a limit that refuses too much, or that forgets what was freed, fails
    // these. The second run allocates 200 MiB through a 8 MiB heap, one string at a time.
    let limits = Limits {
        heap_bytes: 8 * 1024 * 1024,
        ..Limits::default()
    };
    let step = finish(
        "const a = 'x'.repeat(4 * 1024 * 1024); return a.length;",
        limits.clone(),
    )
    .await;
    assert_eq!(done(step), json!(4 * 1024 * 1024));
    let step = finish(
        "for (let i = 0; i < 2000; i++) { const a = 'x'.repeat(100_000); } return 'done';",
        limits,
    )
    .await;
    assert_eq!(done(step), json!("done"));
}

#[tokio::test]
async fn a_script_message_is_not_a_limit() {
    // A class of its own is read from the engine's error, never from the words in it: a script
    // that copies a limit's message has still just thrown, and stays a runtime error.
    for message in [
        "The script used more than its CPU time.",
        "The script used more than its memory limit (heap).",
        "interrupted",
        "out of memory",
        "Maximum call stack size exceeded",
    ] {
        let (kind, text) =
            failure(&format!("throw new Error({message:?});"), Limits::default()).await;
        assert_eq!(kind, ErrorKind::Runtime, "{message}: {text}");
        assert!(text.contains(message), "{message}: {text}");
    }
}

#[tokio::test]
async fn a_getter_on_the_thrown_error_still_ends_at_the_cpu_limit() {
    // Classifying an exception reads its properties, and a getter there is the script's own code:
    // a loop in it must end at the CPU limit like any other, not hold the executor past both.
    let limits = Limits {
        cpu: Duration::from_millis(100),
        ..Limits::default()
    };
    let code = "const e = new Error('boom');\n\
                Object.defineProperty(e, 'name', { get() { while (true) {} } });\n\
                throw e;";
    let (kind, text) = failure(code, limits).await;
    assert_eq!(kind, ErrorKind::Limit, "{text}");
    assert!(text.to_lowercase().contains("cpu"), "{text}");
}

#[tokio::test]
async fn a_message_getter_that_answers_once_still_reports_the_script_error() {
    // The class and the words come from one read of the exception: a getter that answers something else the second
    // time cannot make a heap refusal's retained `heap_hit` replace the message the script threw.
    let limits = Limits {
        heap_bytes: 4 * 1024 * 1024,
        ..Limits::default()
    };
    let code = "try { 'x'.repeat(64 * 1024 * 1024); } catch (e) {}\n\
                let reads = 0;\n\
                const error = new Error('recovered');\n\
                Object.defineProperty(error, 'message', { get() { return ++reads === 1 ? 'recovered' : ''; } });\n\
                Object.defineProperty(error, 'stack', { value: '' });\n\
                throw error;";
    let (kind, text) = failure(code, limits).await;
    assert_eq!(kind, ErrorKind::Runtime, "{text}");
    assert!(text.contains("recovered"), "{text}");
}

#[tokio::test]
async fn a_message_getter_that_answers_late_cannot_change_the_class() {
    // The first read of the message is the only one: an exception whose message reads empty is reasonless for the
    // heap that refused, and the words shown must not come from a later read that answered something else.
    let limits = Limits {
        heap_bytes: 4 * 1024 * 1024,
        ..Limits::default()
    };
    let code = "try { 'x'.repeat(64 * 1024 * 1024); } catch (e) {}\n\
                let reads = 0;\n\
                const error = new Error();\n\
                Object.defineProperty(error, 'message', { get() { return ++reads === 1 ? '' : 'late'; } });\n\
                Object.defineProperty(error, 'stack', { value: '' });\n\
                throw error;";
    let (kind, text) = failure(code, limits).await;
    assert_eq!(kind, ErrorKind::Limit, "{text}");
    assert!(text.to_lowercase().contains("heap"), "{text}");
    assert!(
        !text.contains("late"),
        "no second read decided this: {text}"
    );
}

#[tokio::test]
async fn a_thrown_error_keeps_its_message_when_its_name_cannot_be_read() {
    // A heap refusal is already recorded, but the script's own error explains itself: a `name` the
    // engine cannot read must not turn that message into the heap's diagnostic.
    let limits = Limits {
        heap_bytes: 4 * 1024 * 1024,
        ..Limits::default()
    };
    let code = "try { 'x'.repeat(64 * 1024 * 1024); } catch (e) {}\n\
                const e = new Error('recovered');\n\
                Object.defineProperty(e, 'name', { value: undefined });\n\
                throw e;";
    let (kind, text) = failure(code, limits).await;
    assert_eq!(kind, ErrorKind::Runtime, "{text}");
    assert!(text.contains("recovered"), "{text}");
}

#[tokio::test]
async fn a_full_heap_ends_the_run_instead_of_the_wall_clock() {
    // The fill breaks on an allocation the heap refused, so the refusal is recorded and the data
    // stays live. A heap this full can leave the engine unable to allocate the job that would
    // settle the promise the script awaits (QuickJS throws `null` when it cannot build its own
    // error), so the future stays pending with nothing left that could run it. Before the drive
    // rule the wall clock ended this run and reported a time limit; the heap is what stopped it.
    let code = "globalThis.a = [];\n\
        while (true) { try { globalThis.a.push('x'.repeat(1000)); } catch (e) { break; } }\n\
        await new Promise(() => {});";
    let limits = Limits {
        heap_bytes: 4 * 1024 * 1024,
        // Small on purpose: the assertion is about which rule ends the run, not about wall time.
        wall: Duration::from_secs(2),
        ..Limits::default()
    };
    let (mut run, _) = start(code, limits).await;
    let began = Instant::now();
    let step = run.drive(Instant::now() + Duration::from_secs(60)).await;
    let elapsed = began.elapsed();
    let error = failed(step);
    assert_eq!(error.kind, ErrorKind::Limit);
    assert!(error.message.contains("heap"), "{}", error.message);
    assert!(elapsed < Duration::from_secs(1), "took {elapsed:?}");
}

#[tokio::test]
async fn a_full_heap_after_a_tool_call_ends_the_run_instead_of_the_wall_clock() {
    // The same heap ending, reached after a tool call: the refusal is recorded while the tool is in
    // flight, so the check before the wait cannot tell the heap from a run waiting on a tool. Once
    // the reply lands the script catches the refusal and awaits a promise it can never settle, and
    // only the checks made while it waits can end the run — the wall clock would report a time
    // limit and hold the runtime and the heap until then.
    let code = "await tools.slow({ ms: 100 });\n\
        try { 'x'.repeat(64 * 1024 * 1024); } catch (e) {}\n\
        await new Promise(() => {});";
    let limits = Limits {
        heap_bytes: 4 * 1024 * 1024,
        // Small on purpose: the assertion is about which rule ends the run, and when.
        wall: Duration::from_secs(2),
        ..Limits::default()
    };
    let (mut run, _) = start(code, limits).await;
    let began = Instant::now();
    let step = run.drive(Instant::now() + Duration::from_secs(60)).await;
    let elapsed = began.elapsed();
    let error = failed(step);
    assert_eq!(error.kind, ErrorKind::Limit);
    assert!(error.message.contains("heap"), "{}", error.message);
    assert!(elapsed < Duration::from_secs(1), "took {elapsed:?}");
}

#[tokio::test]
async fn a_pending_run_under_its_heap_still_ends_at_the_wall_clock() {
    // The same unsettleable promise with the heap untouched: the wall clock is still the rule that
    // ends a run which stops making progress, so the heap rule did not take its place.
    let limits = Limits {
        wall: Duration::from_millis(200),
        ..Limits::default()
    };
    let (mut run, _) = start("await new Promise(() => {});", limits).await;
    let error = failed(run.drive(Instant::now() + Duration::from_secs(30)).await);
    assert_eq!(error.kind, ErrorKind::Limit);
    assert!(error.message.contains("time limit"), "{}", error.message);
}

#[tokio::test]
async fn a_heap_refusal_does_not_end_a_run_waiting_on_a_tool() {
    // A refusal the script caught leaves the refusal recorded; the script is still waiting on a
    // tool, so the run yields and resumes as before instead of ending as the heap.
    let limits = Limits {
        heap_bytes: 4 * 1024 * 1024,
        ..Limits::default()
    };
    let code = "try { 'x'.repeat(64 * 1024 * 1024); } catch (e) {}\n\
        await tools.slow({ ms: 400 });\n\
        return 'slept';";
    let (mut run, _) = start(code, limits).await;
    let first = run.drive(Instant::now() + Duration::from_millis(40)).await;
    assert!(matches!(first, Step::Yielded), "{first:?}");
    let second = run.drive(Instant::now() + Duration::from_secs(30)).await;
    assert_eq!(done(second), json!("slept"));
}
