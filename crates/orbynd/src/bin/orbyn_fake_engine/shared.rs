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
