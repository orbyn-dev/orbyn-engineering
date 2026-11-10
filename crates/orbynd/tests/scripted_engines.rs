//! The browser suite's Node engines through the production process resolver.
mod support;

use std::{ffi::OsString, path::Path, process::Stdio, time::Duration};

use orbyn_core::host::process::{ManagedChild, engine_command, resolve};
use serde_json::{Value, json};
use support::{FAKE, TempDir};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn engine(dir: &Path, name: &str, args: &[&str]) -> ManagedChild {
    // Spaces and a different cwd catch shims that accidentally rely on either.
    let scripts = dir.join("test engines");
    std::fs::create_dir_all(&scripts).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/e2e/real");
    for file in [
        format!("{name}.mjs"),
        format!("{name}.cmd"),
        "scenes.mjs".into(),
    ] {
        std::fs::copy(source.join(&file), scripts.join(file)).unwrap();
    }
    let extension = if cfg!(windows) { "cmd" } else { "mjs" };
    let command = resolve(
        scripts
            .join(format!("{name}.{extension}"))
            .to_str()
            .unwrap(),
    )
    .expect("test engine should resolve");
    #[cfg(windows)]
    {
        assert!(
            command
                .program
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("node.exe")),
            "{command:?}"
        );
        assert_eq!(
            command.prefix,
            [scripts.join(format!("{name}.mjs")).into_os_string()]
        );
    }
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut command = engine_command(&command, &args, dir);
    command.env_clear().stderr(Stdio::inherit());
    for key in ["PATH", "Path", "SystemRoot", "WINDIR", "PATHEXT"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    for key in [
        "HOME",
        "USERPROFILE",
        "CODEX_HOME",
        "CLAUDE_CONFIG_DIR",
        "TEMP",
        "TMP",
    ] {
        command.env(key, dir);
    }
    command
        .env("ORBYN_E2E_FAKE", FAKE)
        .env("FAKE_VERSION", "0.166.0");
    ManagedChild::spawn(command).expect("Node test engine should start")
}

#[cfg(windows)]
#[test]
fn a_shebang_script_is_not_a_windows_executable() {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/e2e/real/scripted-engine.mjs");
    let error = std::process::Command::new(script)
        .arg("--version")
        .spawn()
        .expect_err("CreateProcess cannot execute a Node shebang");
    assert_eq!(error.raw_os_error(), Some(193));
}

#[tokio::test]
async fn both_script_engines_forward_version_arguments() {
    let dir = TempDir::new();
    for name in ["scripted-engine", "usage-engine"] {
        let output = tokio::time::timeout(
            Duration::from_secs(10),
            engine(dir.path(), name, &["--version"]).output(),
        )
        .await
        .expect("version probe timed out")
        .unwrap();
        assert!(output.status.success(), "{name}: {output:?}");
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "0.166.0");
    }
}

#[tokio::test]
async fn scripted_engine_initializes_and_finishes_the_benchmark_scene() {
    let dir = TempDir::new();
    let mut child = engine(dir.path(), "scripted-engine", &["app-server"]);
    let mut input = child.child_mut().stdin.take().unwrap();
    let mut output = BufReader::new(child.child_mut().stdout.take().unwrap()).lines();
    let exchange = async {
        for request in [
            json!({"id":1,"method":"initialize","params":{}}),
            json!({"id":2,"method":"thread/start","params":{}}),
        ] {
            input
                .write_all(format!("{request}\n").as_bytes())
                .await
                .unwrap();
            let response: Value =
                serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(response["id"], request["id"]);
            assert!(response.get("result").is_some(), "{response}");
        }
        let request = json!({"id":3,"method":"turn/start","params":{
            "cwd":dir.path(), "input":[{"type":"text","text":"[scene:bench]"}]
        }});
        input
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        let mut text = String::new();
        let mut deltas = 0;
        loop {
            let message: Value = serde_json::from_str(
                &output
                    .next_line()
                    .await
                    .unwrap()
                    .expect("engine exited before turn completed"),
            )
            .unwrap();
            match message["method"].as_str() {
                Some("item/agentMessage/delta") => {
                    text.push_str(message["params"]["delta"].as_str().unwrap());
                    deltas += 1;
                }
                Some("turn/completed") => {
                    assert_eq!(message["params"]["turn"]["status"], "completed");
                    assert!(!message["params"]["threadId"].as_str().unwrap().is_empty());
                    break;
                }
                _ => {}
            }
        }
        assert_eq!(deltas, 200);
        assert_eq!(text.len(), 50 * 1024);
        assert!(text.ends_with("END."));
    };
    tokio::time::timeout(Duration::from_secs(15), exchange)
        .await
        .expect("scripted protocol timed out");
    // The process-tree owner also closes the script's protocol-double descendant.
    drop(input);
    drop(child);
}
