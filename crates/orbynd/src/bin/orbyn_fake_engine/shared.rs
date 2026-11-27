use std::io::{BufRead, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// An interactive console on a session: says which, then echoes lines until input ends.
pub(crate) fn console(mode: &str, args: &[String]) {
    record_launch(mode, args);

    println!("fake console on {}", args.get(1).map_or("", String::as_str));

    let mut line = String::new();

    while std::io::stdin()
        .read_line(&mut line)
        .is_ok_and(|read| read > 0)
    {
        println!("console heard {}", line.trim());

        line.clear();
    }
}

/// Lets a test watch this process's lifetime.
/// Lets a test assert how an engine was launched.
pub(crate) fn record_launch(mode: &str, args: &[String]) {
    if let Ok(path) = std::env::var("ORBYN_FAKE_LAUNCH_FILE") {
        let entry = json!({
            "pid": std::process::id(),

            "mode": mode,

            "args": args,

            "cwd": std::env::current_dir().expect("cwd"),

            // The one engine variable a test reads by value: the run's opt-in to the
            // task tools. Every other name is recorded without its value.
            "task_tools": std::env::var("CLAUDE_CODE_ENABLE_TODO_TOOLS").ok(),

            "env": std::env::vars_os()

                .map(|(name, _)| name.to_string_lossy().into_owned())

                .filter(|name| ["CLAUDE", "ANTHROPIC", "CODEX_S", "OPENAI"]

                    .iter()

                    .any(|prefix| name.starts_with(prefix)))

                .collect::<Vec<_>>(),

        });

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .expect("launch file");

        // Native probes and a run can start together. Formatting into a File

        // uses several writes, so each whole record needs a process-wide lock.

        file.lock().expect("launch file lock");

        let mut record = serde_json::to_vec(&entry).expect("launch JSON");

        record.push(b'\n');

        file.write_all(&record).expect("launch write");
    }
}

pub(crate) fn record_pid() {
    if let Ok(path) = std::env::var("ORBYN_FAKE_PID_FILE") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("pid file");

        writeln!(file, "{}", std::process::id()).expect("pid write");
    }
}

pub(crate) fn record_claude_control(frame: &Value) {
    if let Ok(path) = std::env::var("ORBYN_FAKE_CLAUDE_CONTROL_FILE") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();

        writeln!(file, "{frame}").unwrap();
    }
}

pub(crate) fn send(value: &Value) {
    let mut out = std::io::stdout().lock();

    writeln!(out, "{value}").expect("stdout");

    out.flush().expect("flush");
}

pub(crate) fn stdin_lines() -> impl Iterator<Item = Value> {
    std::io::stdin()
        .lock()
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
}

pub(crate) fn has(prompt: &str, tag: &str) -> bool {
    prompt.contains(&format!("[{tag}]"))
}

/// Plain HTTP POST, enough to reach Orbyn's MCP endpoint without a client stack.
pub(crate) fn http_post(url: &str, body: &Value) -> (u16, Value) {
    let rest = url.strip_prefix("http://").expect("http url");

    let (host, path) = rest.split_once('/').expect("path");

    let mut stream = TcpStream::connect(host).expect("connect");

    let body = body.to_string();

    write!(

        stream,

        "POST /{path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",

        body.len()

    )

    .expect("write");

    let mut response = String::new();

    stream.read_to_string(&mut response).expect("read");

    let (head, text) = response.split_once("\r\n\r\n").expect("response");

    let status = head
        .split_whitespace()
        .nth(1)
        .expect("status")
        .parse()
        .expect("status");

    (status, serde_json::from_str(text).unwrap_or(Value::Null))
}

/// How long one attempt may take. A refusal is answered in milliseconds; a service busy under load
/// may take longer, and one that never answers must reach the test as an error for that route, not
/// as an engine that hangs and writes nothing.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(5);

/// How much of the test's minute the whole sweep may use. It ends the sweep with a report that says
/// what it reached, so the report always arrives before the test stops waiting for it: this, one
/// more attempt, and the process exiting stay inside that minute.
const PROBE_BUDGET: Duration = Duration::from_secs(40);

/// Writes the report where the test reads it, whole: a reader never sees half a JSON document.
fn write_report(path: &Path, report: &Value) {
    let temporary = path.with_extension("json.tmp");
    if std::fs::write(&temporary, serde_json::to_vec(report).expect("probe JSON")).is_ok() {
        let _ = std::fs::rename(&temporary, path);
    }
}

/// What a worker that runs as the person can do (#441): read `<data>/service.json` and call the routes that need the
/// person key, with the service token alone. The routes come from `ORBYN_FAKE_PROBE_ROUTES`, a JSON array of
/// `[method, path, body]` (the test fills the path parameters in); without it the probe calls the file-action, approval and
/// AG-UI resume routes.
/// Writes each status and error code, and this process's own argv and env, to `ORBYN_FAKE_PROBE_FILE`. The report is
/// written whole after every call with `complete: false` and the route it is on, then once with `complete: true`, so a
/// sweep a slow service is still holding says how far it got instead of leaving the test a file that never appeared.
/// Does nothing unless a test sets `ORBYN_FAKE_PROBE_FILE` and `ORBYN_FAKE_PROBE_DATA`.
pub(crate) fn probe_service(args: &[String]) {
    let (Ok(out), Ok(_)) = (
        std::env::var("ORBYN_FAKE_PROBE_FILE"),
        std::env::var("ORBYN_FAKE_PROBE_DATA"),
    ) else {
        return;
    };
    // A failure in here must reach the test as a reason, not as a missing file: a panic is caught and written as `fatal`.
    let report = match std::panic::catch_unwind(|| probe_report(args)) {
        Ok(report) => report,
        Err(panic) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "a panic with no message".to_owned());
            json!({
                "calls": [],
                "complete": true,
                "fatal": format!("the probe panicked: {message}"),
                "argv": args,
            })
        }
    };
    write_report(Path::new(&out), &report);
}

/// Hides the token and any long token-looking run in text the probe reports.
fn redact(text: &str, token: &str) -> String {
    let text = text.replace(token, "[redacted]");
    let mut out = String::new();
    let mut run = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            run.push(c);
            continue;
        }
        out.push_str(if run.len() >= 32 { "[redacted]" } else { &run });
        run.clear();
        out.push(c);
    }
    out.pop();
    out.chars().take(500).collect()
}

/// One request over a fresh connection: the status and the response text, or why it failed.
fn probe_once(
    host: &str,
    token: &str,
    method: &str,
    path: &str,
    body: &str,
) -> Result<(u16, String), String> {
    // Every step is bounded: an answer that never comes is this route's error, retried like any
    // other transport error, and never a wait with no end.
    let address: SocketAddr = host
        .parse()
        .map_err(|error| format!("address {host}: {error}"))?;
    let mut stream = TcpStream::connect_timeout(&address, ATTEMPT_TIMEOUT)
        .map_err(|error| format!("connect: {error}"))?;
    stream
        .set_read_timeout(Some(ATTEMPT_TIMEOUT))
        .map_err(|error| format!("read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(ATTEMPT_TIMEOUT))
        .map_err(|error| format!("write timeout: {error}"))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|error| format!("write: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("read: {e}"))?;
    let (head, text) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("no header end in {} bytes", response.len()))?;
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("no status in {head:?}"))?;
    Ok((status, text.to_owned()))
}

fn probe_report(args: &[String]) -> Value {
    let data = std::env::var("ORBYN_FAKE_PROBE_DATA").expect("ORBYN_FAKE_PROBE_DATA");
    let out = std::env::var("ORBYN_FAKE_PROBE_FILE").expect("ORBYN_FAKE_PROBE_FILE");
    let out = Path::new(&out);
    let found: Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&data).join("service.json"))
            .expect("service.json"),
    )
    .expect("service.json is JSON");
    let url = found["url"].as_str().expect("url");
    let token = found["token"].as_str().expect("token");
    let routes: Vec<(String, String, String)> = match std::env::var("ORBYN_FAKE_PROBE_ROUTES") {
        Ok(text) => serde_json::from_str(&text).expect("probe routes"),
        Err(_) => {
            // The AG-UI resume answers an interrupt, so it is the third way to approve and needs the key too.
            let resume = json!({
                "threadId": "t", "runId": "r",
                "resume": [{ "interruptId": "a", "status": "resolved", "payload": { "approved": true } }],
            })
            .to_string();
            vec![
                (
                    "POST".into(),
                    "/api/projects/p/worktrees/w/file-actions".into(),
                    "{}".into(),
                ),
                ("POST".into(), "/api/approvals/a/decide".into(), "{}".into()),
                ("POST".into(), "/agui/threads/t".into(), resume),
            ]
        }
    };
    let host = url.strip_prefix("http://").expect("http url");
    let began = Instant::now();
    let mut calls: Vec<Value> = Vec::new();
    let mut aborted = None;
    for (method, path, body) in &routes {
        // The test reads this while the sweep runs, so a probe a slow service still holds says which
        // route it reached and which one it is on.
        write_report(
            out,
            &json!({ "calls": calls, "in_flight": format!("{method} {path}"), "complete": false }),
        );
        // A transport error (connect, write, read) is tried again, so a service busy under load is not a failure by
        // itself; an answer of any status is never tried again. The headers are named, never valued.
        let mut attempt = 0;
        let outcome = loop {
            if began.elapsed() > PROBE_BUDGET {
                break Err(format!(
                    "no answer within the probe's {}s budget",
                    PROBE_BUDGET.as_secs()
                ));
            }
            attempt += 1;
            match probe_once(host, token, method, path, body) {
                Ok(answer) => break Ok(answer),
                Err(error) if attempt >= 3 => break Err(error),
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        };
        let request =
            format!("{method} {url}{path} (headers: authorization, content-type, content-length)");
        calls.push(match outcome {
            Ok((status, text)) => {
                let parsed: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                json!({
                    "method": method, "path": path, "status": status, "code": parsed["code"],
                    "request": request, "attempt": attempt, "body": redact(&text, token),
                })
            }
            Err(error) => json!({
                "method": method, "path": path, "request": request, "attempt": attempt,
                "error": error,
            }),
        });
        if began.elapsed() > PROBE_BUDGET {
            aborted = Some(format!(
                "{} of {} calls done in {}s",
                calls.len(),
                routes.len(),
                began.elapsed().as_secs()
            ));
            break;
        }
    }
    let env: Vec<String> = std::env::vars_os()
        .map(|(name, value)| format!("{}={}", name.to_string_lossy(), value.to_string_lossy()))
        .collect();
    json!({ "calls": calls, "argv": args, "env": env, "complete": true, "aborted": aborted })
}

pub(crate) fn mcp_call(url: &str, id: u64, method: &str, params: Value) -> (u16, Value) {
    http_post(
        url,
        &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
    )
}

/// Lets a test change the engine's answers while it runs.
pub(crate) fn sibling_exists(name: &str) -> bool {
    std::env::var("ORBYN_FAKE_LAUNCH_FILE")
        .is_ok_and(|path| std::path::Path::new(&path).with_file_name(name).exists())
}

/// Lets this engine session leave a state a later one should see, such as an account
/// window it spent. Written beside the launch file, the same place a test writes its knobs.
pub(crate) fn write_sibling(name: &str, text: &str) {
    if let Ok(path) = std::env::var("ORBYN_FAKE_LAUNCH_FILE") {
        std::fs::write(std::path::Path::new(&path).with_file_name(name), text)
            .expect("sibling file write");
    }
}

pub(crate) const WORKFLOW_SCRIPT: &str =
    "export const meta = { name: 'probe', description: 'probe' }\nawait agent('look')\n";

#[cfg(test)]
mod tests {
    use super::*;

    /// The test reads the report while the sweep runs, so a write replaces the file whole: a reader
    /// finds either the report before it or the one after, never half a JSON document.
    #[test]
    fn a_report_is_replaced_whole() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("probe-report-{unique}"));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("probe.json");
        let first = json!({ "calls": [], "in_flight": "POST /a", "complete": false });
        let second = json!({
            "calls": [{ "method": "POST", "path": "/a", "status": 403 }],
            "complete": true,
        });
        write_report(&path, &first);
        assert_eq!(
            serde_json::from_str::<Value>(&std::fs::read_to_string(&path).expect("report"))
                .expect("JSON"),
            first
        );
        write_report(&path, &second);
        assert_eq!(
            serde_json::from_str::<Value>(&std::fs::read_to_string(&path).expect("report"))
                .expect("JSON"),
            second
        );
        assert!(
            !dir.join("probe.json.tmp").exists(),
            "the temporary file is renamed, not left behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
