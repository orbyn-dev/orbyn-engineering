use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use serde_json::{Value, json};

use crate::shared::has;

// AG-UI over HTTP and SSE.

pub(crate) fn agui(args: &[String]) {
    let port_file = args.get(1).expect("port file");

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");

    let port = listener.local_addr().expect("addr").port();

    std::fs::write(port_file, port.to_string()).expect("port file");

    for stream in listener.incoming().map_while(Result::ok) {
        std::thread::spawn(move || agui_connection(stream));
    }
}

pub(crate) fn agui_connection(mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));

    let mut request_line = String::new();

    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    let mut length = 0usize;

    loop {
        let mut line = String::new();

        if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty() {
            break;
        }

        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }

    if !request_line.starts_with("POST") {
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );

        return;
    }

    let mut body = vec![0u8; length];

    if reader.read_exact(&mut body).is_err() {
        return;
    }

    let input: Value = serde_json::from_slice(&body).expect("RunAgentInput");

    for field in ["threadId", "runId", "messages"] {
        assert!(input.get(field).is_some(), "RunAgentInput needs {field}");
    }

    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
    );

    let thread = input["threadId"].clone();

    let run = input["runId"].clone();

    let mut frame = |value: Value| {
        let _ = write!(stream, "data: {value}\n\n");

        let _ = stream.flush();
    };

    frame(json!({ "type": "RUN_STARTED", "threadId": thread, "runId": run }));

    if let Some(resume) = input["resume"].as_array() {
        let approved = resume[0]["payload"]["approved"].as_bool().unwrap_or(false);

        frame(json!({ "type": "TEXT_MESSAGE_START", "messageId": "m2" }));

        frame(json!({

            "type": "TEXT_MESSAGE_CONTENT", "messageId": "m2",

            "delta": format!("resumed approved={approved}"),

        }));

        frame(json!({ "type": "TEXT_MESSAGE_END", "messageId": "m2" }));

        frame(json!({

            "type": "RUN_FINISHED", "threadId": thread, "runId": run,

            "outcome": { "type": "success" },

        }));

        return;
    }

    let prompt = input["messages"][0]["content"].as_str().unwrap_or_default();

    if has(prompt, "approve") {
        frame(json!({ "type": "TOOL_CALL_START", "toolCallId": "t1", "toolCallName": "deploy" }));

        frame(json!({ "type": "TOOL_CALL_END", "toolCallId": "t1" }));

        frame(json!({

            "type": "RUN_FINISHED", "threadId": thread, "runId": run,

            "outcome": { "type": "interrupt", "interrupts": [{

                "id": "int-1", "reason": "tool_approval", "message": "Deploy now?",

                "toolCallId": "t1",

            }]},

        }));

        return;
    }

    if has(prompt, "fail") {
        frame(json!({ "type": "RUN_ERROR", "message": "fake failure", "code": "fake" }));

        return;
    }

    frame(json!({ "type": "TEXT_MESSAGE_START", "messageId": "m1" }));

    frame(json!({ "type": "TEXT_MESSAGE_CONTENT", "messageId": "m1", "delta": "hello from agui" }));

    frame(json!({ "type": "TEXT_MESSAGE_END", "messageId": "m1" }));

    frame(json!({ "type": "STATE_SNAPSHOT", "snapshot": { "n": 1 } }));

    frame(json!({

        "type": "RUN_FINISHED", "threadId": thread, "runId": run,

        "outcome": { "type": "success" },

    }));
}
