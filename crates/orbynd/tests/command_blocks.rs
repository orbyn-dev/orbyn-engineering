//! Command blocks through the real service: the local list and output reads, their
//! empty answer for a shell with no marks, the `404`s for anything gone, and the
//! Person-class clear with its audit row.

mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::{Options, Service, TempDir, setup};

/// The two blocks `blocks` writes: `first\n` (6 bytes) and `second\n` (7 bytes).
const FINISHED_BYTES: usize = 13;

fn desktop() -> Options {
    Options {
        args: vec!["--parent-stdin".into()],
        env: vec![
            ("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into()),
            (
                "ORBYN_FAKE_BLOCKS".into(),
                env!("CARGO_BIN_EXE_orbyn-fake-engine").into(),
            ),
        ],
    }
}

/// A command that runs the fake engine in one block mode.
///
/// A POSIX shell takes the quoted path as it stands, so the command names the engine.
/// Windows runs a project script through `cmd /C` with the whole command line as one
/// argument, and a quote inside that argument cannot reach `cmd` intact, so the fixture
/// writes a one-line batch file beside `orbyn.json` and runs it by its bare name: `cmd`
/// reads the quotes of the batch text itself, and the folder holding the engine may
/// contain spaces.
async fn emitter(service: &Service, thread: &str, mode: &str) -> String {
    if cfg!(windows) {
        let name = format!("emit-{mode}.cmd");
        std::fs::write(
            thread_cwd(service, thread).await.join(&name),
            format!("@echo off\r\n\"%ORBYN_FAKE_BLOCKS%\" {mode}\r\n"),
        )
        .unwrap();
        name
    } else {
        format!("\"$ORBYN_FAKE_BLOCKS\" {mode}")
    }
}

async fn get(service: &Service, path: &str) -> (u16, Value) {
    let response = service.get(path).send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn delete(service: &Service, path: &str, with_key: bool) -> (u16, Value) {
    let mut request = service
        .client
        .delete(format!("{}{path}", service.url))
        .bearer_auth(&service.token);
    if with_key {
        let key = service
            .person_key
            .as_deref()
            .expect("the desktop service has a person key");
        request = request.header("x-orbyn-person", key);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

async fn thread_cwd(service: &Service, thread: &str) -> PathBuf {
    let body = service.json(&format!("/api/threads/{thread}")).await;
    Path::new(body["cwd"].as_str().unwrap()).to_path_buf()
}

async fn write_scripts(service: &Service, thread: &str, scripts: &[(&str, &str)]) {
    let scripts: Vec<Value> = scripts
        .iter()
        .map(|(name, command)| json!({ "name": name, "command": command }))
        .collect();
    std::fs::write(
        thread_cwd(service, thread).await.join("orbyn.json"),
        json!({ "scripts": scripts }).to_string(),
    )
    .unwrap();
}

async fn run_script(service: &Service, thread: &str, name: &str) -> String {
    let (code, info) = service
        .send(
            &format!("/api/threads/{thread}/scripts"),
            json!({ "name": name }),
        )
        .await;
    assert_eq!(code, 201, "{info}");
    info["id"].as_str().unwrap().to_owned()
}

fn events_path(thread: &str, terminal: &str) -> String {
    format!("/api/threads/{thread}/terminals/{terminal}/output/events")
}

/// Waits until the terminal's output ends. The block recorder sees a chunk before the
/// output stream does, so every finished block is recorded by then.
async fn wait_ended(service: &Service, thread: &str, terminal: &str) {
    service
        .sse_until(&events_path(thread, terminal), |event| {
            event["type"] == "ended"
        })
        .await;
}

/// Waits until the running block's output has gone out; the finished block before it
/// was recorded first.
async fn wait_running(service: &Service, thread: &str, terminal: &str) {
    service
        .sse_until(&events_path(thread, terminal), |event| {
            event["text"]
                .as_str()
                .is_some_and(|text| text.contains("still-running"))
        })
        .await;
}

async fn blocks(service: &Service, thread: &str, terminal: &str) -> Value {
    service
        .json(&format!(
            "/api/threads/{thread}/terminals/{terminal}/blocks"
        ))
        .await
}

fn clear_path(thread: &str, terminal: &str) -> String {
    format!("/api/threads/{thread}/terminals/{terminal}/blocks")
}

#[tokio::test]
async fn blocks_answer_not_found_or_an_empty_list() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "codex").await;

    let (code, body) = get(
        &service,
        "/api/threads/no-such-thread/terminals/main/blocks",
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (404, Some("not_found")));

    let (code, body) = get(
        &service,
        &format!("/api/threads/{thread}/terminals/main/blocks"),
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (404, Some("not_found")));

    let (code, terminal) = service
        .send(&format!("/api/threads/{thread}/terminals"), json!({}))
        .await;
    assert_eq!(code, 201, "{terminal}");
    let terminal = terminal["id"].as_str().unwrap();
    let base = format!("/api/threads/{thread}/terminals/{terminal}");

    // A shell that emits no OSC 133 marks yields no blocks, not an error.
    let (code, list) = get(&service, &format!("{base}/blocks")).await;
    assert_eq!(code, 200, "{list}");
    assert_eq!(list["blocks"], json!([]));
    assert!(list["next_before"].is_null());
    assert_eq!(list["limits"]["max_blocks"], json!(500));
    assert_eq!(list["limits"]["max_block_bytes"], json!(1_048_576));
    assert_eq!(list["limits"]["max_total_bytes"], json!(8_388_608));

    let (code, body) = get(&service, &format!("{base}/blocks/no-such-block/output")).await;
    assert_eq!((code, body["code"].as_str()), (404, Some("not_found")));

    let (code, body) = get(&service, &format!("{base}/blocks?before=nope")).await;
    assert_eq!((code, body["code"].as_str()), (422, Some("bad_cursor")));

    let closed = service
        .client
        .delete(format!(
            "{}/api/threads/{thread}/terminals/{terminal}",
            service.url
        ))
        .bearer_auth(&service.token)
        .send()
        .await
        .unwrap();
    assert_eq!(closed.status(), 204);
    let (code, body) = get(&service, &format!("{base}/blocks")).await;
    assert_eq!(
        (code, body["code"].as_str()),
        (404, Some("not_found")),
        "closing the terminal drops its blocks"
    );
    service.stop().await;
}

#[tokio::test]
async fn a_clear_needs_the_person_and_drops_only_that_terminals_finished_blocks() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, desktop(), "codex").await;
    let finished = emitter(&service, &thread, "blocks").await;
    let other_script = emitter(&service, &thread, "blocks").await;
    write_scripts(
        &service,
        &thread,
        &[("finished", &finished), ("other", &other_script)],
    )
    .await;
    let terminal = run_script(&service, &thread, "finished").await;
    let other = run_script(&service, &thread, "other").await;
    wait_ended(&service, &thread, &terminal).await;
    wait_ended(&service, &thread, &other).await;
    let before = blocks(&service, &thread, &terminal).await;
    assert_eq!(before["blocks"].as_array().unwrap().len(), 2);
    assert_eq!(before["blocks"][0]["command"], "echo first");
    assert_eq!(before["blocks"][1]["command"], "echo second");

    let (code, body) = delete(&service, &clear_path(&thread, &terminal), false).await;
    assert_eq!(
        (code, body["code"].as_str()),
        (403, Some("person_required"))
    );
    assert_eq!(blocks(&service, &thread, &terminal).await, before);

    let (code, answer) = delete(&service, &clear_path(&thread, &terminal), true).await;
    assert_eq!(code, 200, "{answer}");
    assert_eq!(answer, json!({ "cleared": 2, "kept_running": 0 }));
    let after = blocks(&service, &thread, &terminal).await;
    assert_eq!(after["blocks"], json!([]));
    assert!(after["next_before"].is_null());
    let untouched = blocks(&service, &thread, &other).await;
    assert_eq!(
        untouched["blocks"].as_array().unwrap().len(),
        2,
        "another terminal's blocks are untouched"
    );

    let audit = service.audit(&thread).await;
    let row = audit
        .iter()
        .find(|row| row["action"] == "terminal.blocks.clear")
        .expect("the clear is audited");
    assert_eq!(row["actor"], "user");
    assert_eq!(row["target"], terminal.as_str());
    assert_eq!(
        row["detail"],
        json!({ "blocks": 2, "bytes": FINISHED_BYTES })
    );
    let row_text = row.to_string();
    assert!(!row_text.contains("echo first"), "{row_text}");
    assert!(!row_text.contains("echo second"), "{row_text}");
    assert!(!row_text.contains("first\\n"), "{row_text}");
    assert!(!row_text.contains("second\\n"), "{row_text}");
    service.stop().await;
}

#[tokio::test]
async fn a_running_block_survives_a_clear_and_its_output_goes_on() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, desktop(), "codex").await;
    let running = emitter(&service, &thread, "blocks-running").await;
    write_scripts(&service, &thread, &[("running", &running)]).await;
    let terminal = run_script(&service, &thread, "running").await;
    wait_running(&service, &thread, &terminal).await;

    let before = blocks(&service, &thread, &terminal).await;
    assert_eq!(before["blocks"].as_array().unwrap().len(), 2);
    assert!(before["blocks"][0]["ended_at"].is_string());
    assert!(before["blocks"][1]["ended_at"].is_null());
    assert_eq!(before["blocks"][1]["output_bytes"], 14);

    let (code, answer) = delete(&service, &clear_path(&thread, &terminal), true).await;
    assert_eq!(code, 200, "{answer}");
    assert_eq!(answer, json!({ "cleared": 1, "kept_running": 1 }));

    let after = blocks(&service, &thread, &terminal).await;
    let kept = after["blocks"].as_array().unwrap();
    assert_eq!(kept.len(), 1);
    assert!(kept[0]["ended_at"].is_null());
    assert_eq!(kept[0]["command"], "sleep");
    assert_eq!(kept[0]["output_bytes"], 14);
    let output = service
        .json(&format!(
            "/api/threads/{thread}/terminals/{terminal}/blocks/{}/output",
            kept[0]["id"].as_str().unwrap()
        ))
        .await;
    assert_eq!(output["text"], "still-running\n");

    let (code, again) = delete(&service, &clear_path(&thread, &terminal), true).await;
    assert_eq!(code, 200, "{again}");
    assert_eq!(again, json!({ "cleared": 0, "kept_running": 1 }));
    let audit = service.audit(&thread).await;
    let rows: Vec<&Value> = audit
        .iter()
        .filter(|row| row["action"] == "terminal.blocks.clear")
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["detail"], json!({ "blocks": 0, "bytes": 0 }));
    assert_eq!(rows[1]["detail"], json!({ "blocks": 1, "bytes": 5 }));
    assert_eq!(rows[0]["target"], terminal.as_str());
    service.stop().await;
}

#[tokio::test]
async fn a_clear_answers_not_found_desktop_required_or_zero() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, desktop(), "codex").await;

    let (code, body) = delete(
        &service,
        "/api/threads/no-such-thread/terminals/main/blocks",
        true,
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (404, Some("not_found")));
    let (code, body) = delete(&service, &clear_path(&thread, "no-such-terminal"), true).await;
    assert_eq!((code, body["code"].as_str()), (404, Some("not_found")));

    // A terminal that recorded nothing clears to zero.
    let (code, terminal) = service
        .send(&format!("/api/threads/{thread}/terminals"), json!({}))
        .await;
    assert_eq!(code, 201, "{terminal}");
    let terminal = terminal["id"].as_str().unwrap();
    let (code, answer) = delete(&service, &clear_path(&thread, terminal), true).await;
    assert_eq!(code, 200, "{answer}");
    assert_eq!(answer, json!({ "cleared": 0, "kept_running": 0 }));
    assert_eq!(
        blocks(&service, &thread, terminal).await["blocks"],
        json!([])
    );

    // A service with no desktop shell has no person key at all.
    let shell_less = Service::start(
        &dir.path().join("shell-less"),
        Options {
            args: Vec::new(),
            env: vec![("ORBYN_DEV_DISABLE_PERSON_KEY".into(), "0".into())],
        },
    );
    assert!(shell_less.person_key.is_none());
    let (code, body) = delete(&shell_less, &clear_path(&thread, "main"), false).await;
    assert_eq!(
        (code, body["code"].as_str()),
        (403, Some("desktop_required"))
    );
    assert_eq!(
        body["message"],
        "This action is only available in the desktop app."
    );
    shell_less.stop().await;
    service.stop().await;
}
