use std::io::{BufRead, Read, Write};
use std::net::TcpStream;

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

/// What a worker that runs as the person can do (#441): read `<data>/service.json` and call the routes that need the
/// person key, with the service token alone. The routes come from `ORBYN_FAKE_PROBE_ROUTES`, a JSON array of
/// `[method, path, body]` (the test fills the path parameters in); without it the probe calls the file-action, approval and
/// AG-UI resume routes.
/// Writes each status and error code, and this process's own argv and env, to `ORBYN_FAKE_PROBE_FILE`. Does nothing
/// unless a test sets `ORBYN_FAKE_PROBE_FILE` and `ORBYN_FAKE_PROBE_DATA`.
pub(crate) fn probe_service(args: &[String]) {
    let (Ok(out), Ok(data)) = (
        std::env::var("ORBYN_FAKE_PROBE_FILE"),
        std::env::var("ORBYN_FAKE_PROBE_DATA"),
    ) else {
        return;
    };
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
    let calls: Vec<Value> = routes
        .iter()
        .map(|(method, path, body)| {
            let host = url.strip_prefix("http://").expect("http url");
            let mut stream = TcpStream::connect(host).expect("connect");
            write!(
                stream,
                "{method} {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write");
            let mut response = String::new();
            stream.read_to_string(&mut response).expect("read");
            let (head, text) = response.split_once("\r\n\r\n").expect("response");
            let status: u16 = head
                .split_whitespace()
                .nth(1)
                .expect("status")
                .parse()
                .expect("status");
            let body: Value = serde_json::from_str(text).unwrap_or(Value::Null);
            json!({ "method": method, "path": path, "status": status, "code": body["code"] })
        })
        .collect();
    let env: Vec<String> = std::env::vars_os()
        .map(|(name, value)| format!("{}={}", name.to_string_lossy(), value.to_string_lossy()))
        .collect();
    let report = json!({ "calls": calls, "argv": args, "env": env });
    std::fs::write(&out, serde_json::to_vec(&report).expect("probe JSON")).expect("probe file");
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

pub(crate) const WORKFLOW_SCRIPT: &str =
    "export const meta = { name: 'probe', description: 'probe' }\nawait agent('look')\n";
