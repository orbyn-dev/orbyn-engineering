//! The Codex worker's private home, its auth rule and the profile that closes the real home.

use std::path::{Path, PathBuf};

use super::Confinement;
use super::codex::{self, AUTH_REFUSED, AuthKind};

struct Fixture {
    _dir: crate::testing::TempDir,
    base: PathBuf,
    data: PathBuf,
    real: PathBuf,
}

fn fixture() -> Fixture {
    let dir = crate::testing::TempDir::new();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let data = base.join("data");
    for folder in ["runs/mine", "workspace/repo/p", "workspace/lib"] {
        std::fs::create_dir_all(data.join(folder)).unwrap();
    }
    let real = base.join("real-codex");
    for folder in ["sessions", "log"] {
        std::fs::create_dir_all(real.join(folder)).unwrap();
    }
    for (file, text) in [
        (
            "auth.json",
            r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-fake-api-key"}"#,
        ),
        ("config.toml", "model = \"fake\""),
        ("history.jsonl", "old prompt"),
        ("sessions/rollout.jsonl", "old session"),
        ("state.sqlite3", "old state"),
        ("log/codex.log", "old log"),
    ] {
        std::fs::write(real.join(file), text).unwrap();
    }
    Fixture {
        _dir: dir,
        base,
        data,
        real,
    }
}

fn rules(data: &Path) -> Confinement {
    Confinement::new(
        data,
        &data.join("runs/mine"),
        &[&data.join("workspace/lib")],
        &[&data.join("workspace/repo/p")],
    )
    .unwrap()
}

const TASK: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn only_a_single_safe_task_id_can_name_a_home() {
    for bad in ["", ".", "..", "a/b", "a\\b", "a b", "a.b", "a\0b"] {
        assert!(codex::home_name(bad).is_err(), "{bad:?}");
    }
    let name = codex::home_name(TASK).unwrap();
    assert!(!name.contains('/') && !name.contains('\\'), "{name}");
}

#[test]
fn the_auth_table_accepts_only_a_complete_api_key_document() {
    let cases = [
        (
            r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-fake"}"#,
            AuthKind::ApiKey,
        ),
        (r#"{"auth_mode":"apikey"}"#, AuthKind::Unknown),
        (
            r#"{"auth_mode":"apikey","OPENAI_API_KEY":""}"#,
            AuthKind::Unknown,
        ),
        (
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"fake"}}"#,
            AuthKind::ChatGpt,
        ),
        (r#"{"tokens":{"access_token":"fake"}}"#, AuthKind::Unknown),
        (r#"{"auth_mode":"something-new"}"#, AuthKind::Unknown),
        ("", AuthKind::Unknown),
        ("not json", AuthKind::Unknown),
    ];
    for (document, expected) in cases {
        assert_eq!(
            codex::auth_kind(document.as_bytes()),
            expected,
            "{document}"
        );
        let result = codex::check_auth(document.as_bytes());
        if expected == AuthKind::ApiKey {
            assert!(result.is_ok(), "{document}");
        } else {
            assert_eq!(result.unwrap_err().to_string(), AUTH_REFUSED, "{document}");
        }
    }
}

#[test]
fn an_auth_error_never_contains_the_document() {
    const MARKER: &str = "sk-FAKE-CONTENT-MARKER";
    let document = format!(
        r#"{{"auth_mode":"chatgpt","tokens":{{"access_token":"{MARKER}"}},"last_refresh":"{MARKER}"}}"#
    );
    let error = codex::check_auth(document.as_bytes())
        .unwrap_err()
        .to_string();
    assert_eq!(error, AUTH_REFUSED);
    assert!(!error.contains(MARKER), "{error}");
}

#[test]
fn a_private_home_holds_only_the_auth_file_with_private_modes() {
    let f = fixture();
    let configured = rules(&f.data)
        .with_codex_worker_home_at(TASK, &f.real)
        .unwrap();
    let home = configured.codex_home().unwrap().to_path_buf();
    assert_eq!(home, codex::home_path(&f.data, TASK).unwrap());
    assert!(home.is_dir());
    let mut names: Vec<String> = std::fs::read_dir(&home)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["auth.json"]);
    let copied: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.join("auth.json")).unwrap()).unwrap();
    assert_eq!(
        copied,
        serde_json::json!({"auth_mode": "apikey", "OPENAI_API_KEY": "sk-fake-api-key"})
    );
    // The real home keeps everything; nothing else is copied.
    for kept in ["config.toml", "history.jsonl", "sessions/rollout.jsonl"] {
        assert!(f.real.join(kept).is_file(), "{kept}");
    }
    assert!(configured.allow_write.contains(&home));
    assert!(configured.closed.contains(&f.real));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for folder in [
            home.as_path(),
            home.parent().unwrap(),
            home.parent().unwrap().parent().unwrap(),
        ] {
            let mode = std::fs::metadata(folder).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{folder:?} {mode:o}");
        }
        let mode = std::fs::metadata(home.join("auth.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn a_chatgpt_sign_in_refuses_the_worker_before_any_home_exists() {
    let f = fixture();
    const MARKER: &str = "sk-FAKE-CHATGPT-MARKER";
    std::fs::write(
        f.real.join("auth.json"),
        format!(r#"{{"auth_mode":"chatgpt","tokens":{{"access_token":"{MARKER}"}}}}"#),
    )
    .unwrap();
    let error = rules(&f.data)
        .with_codex_worker_home_at(TASK, &f.real)
        .unwrap_err()
        .to_string();
    assert_eq!(error, AUTH_REFUSED);
    assert!(!error.contains(MARKER), "{error}");
    assert!(!codex::home_path(&f.data, TASK).unwrap().exists());
}

#[test]
fn a_missing_or_unreadable_auth_file_is_a_plain_refusal() {
    let f = fixture();
    std::fs::remove_file(f.real.join("auth.json")).unwrap();
    let error = rules(&f.data)
        .with_codex_worker_home_at(TASK, &f.real)
        .unwrap_err()
        .to_string();
    assert!(!error.is_empty());
    assert!(!error.contains("sk-"), "{error}");
    assert!(!codex::home_path(&f.data, TASK).unwrap().exists());
}

#[cfg(unix)]
#[test]
fn a_symlinked_auth_file_is_never_followed() {
    let f = fixture();
    let elsewhere = f.base.join("elsewhere-auth");
    const MARKER: &str = "sk-FAKE-SYMLINK-MARKER";
    std::fs::write(
        &elsewhere,
        format!(r#"{{"auth_mode":"apikey","OPENAI_API_KEY":"{MARKER}"}}"#),
    )
    .unwrap();
    std::fs::remove_file(f.real.join("auth.json")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, f.real.join("auth.json")).unwrap();
    let error = rules(&f.data)
        .with_codex_worker_home_at(TASK, &f.real)
        .unwrap_err()
        .to_string();
    assert!(!error.contains(MARKER), "{error}");
    assert!(!codex::home_path(&f.data, TASK).unwrap().exists());
}

#[cfg(unix)]
#[test]
fn a_linked_or_plain_file_home_path_is_refused() {
    // A linked `crew` folder.
    let f = fixture();
    let elsewhere = f.base.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, f.data.join("crew")).unwrap();
    assert!(
        rules(&f.data)
            .with_codex_worker_home_at(TASK, &f.real)
            .is_err()
    );
    // A linked home for this task.
    let f = fixture();
    let root = f.data.join("crew/codex-homes");
    std::fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join(codex::home_name(TASK).unwrap())).unwrap();
    assert!(
        rules(&f.data)
            .with_codex_worker_home_at(TASK, &f.real)
            .is_err()
    );
    // A plain file where the home goes.
    let f = fixture();
    let root = f.data.join("crew/codex-homes");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(codex::home_name(TASK).unwrap()), "not a folder").unwrap();
    assert!(
        rules(&f.data)
            .with_codex_worker_home_at(TASK, &f.real)
            .is_err()
    );
}

#[test]
fn a_later_turn_reuses_the_same_home_and_its_session() {
    let f = fixture();
    let first = rules(&f.data)
        .with_codex_worker_home_at(TASK, &f.real)
        .unwrap();
    let home = first.codex_home().unwrap().to_path_buf();
    std::fs::create_dir_all(home.join("sessions")).unwrap();
    std::fs::write(
        home.join("sessions/rollout.jsonl"),
        "the worker's own session",
    )
    .unwrap();
    let second = rules(&f.data)
        .with_codex_worker_home_at(TASK, &f.real)
        .unwrap();
    assert_eq!(second.codex_home().unwrap(), home.as_path());
    assert_eq!(
        std::fs::read(home.join("sessions/rollout.jsonl")).unwrap(),
        b"the worker's own session"
    );
    assert!(home.join("auth.json").is_file());
}

#[test]
fn a_worker_home_cannot_be_its_real_home_or_escape_its_private_folder() {
    let f = fixture();
    let outside = f.base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let root = f.data.join("crew/codex-homes");
    std::fs::create_dir_all(&root).unwrap();
    let mut base = rules(&f.data);
    base.closed = vec![f.real.clone()];

    // Equal to the real home, or inside it.
    for path in [f.real.clone(), f.real.join("nested")] {
        std::fs::create_dir_all(&path).unwrap();
        base.codex_home = Some(path);
        assert!(base.codex_home().is_err());
    }
    // Outside the private folder.
    base.codex_home = Some(outside);
    assert!(base.codex_home().is_err());
    // A valid private home is accepted.
    let configured = rules(&f.data)
        .with_codex_worker_home_at(TASK, &f.real)
        .unwrap();
    assert!(configured.codex_home().is_ok());
}

#[cfg(unix)]
#[test]
fn a_worker_home_that_is_a_link_outside_its_private_folder_is_refused() {
    let f = fixture();
    let outside = f.base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let root = f.data.join("crew/codex-homes");
    std::fs::create_dir_all(&root).unwrap();
    let link = root.join(
        codex::home_name("sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
            .unwrap(),
    );
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let mut base = rules(&f.data);
    base.closed = vec![f.real.clone()];
    base.codex_home = Some(link);
    assert!(base.codex_home().is_err());
}

#[test]
fn a_stale_task_home_is_swept_and_a_live_one_stays() {
    let f = fixture();
    let live = TASK;
    let stale = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let live_home = codex::prepare_home(&f.data, live, &f.real).unwrap();
    let stale_home = codex::prepare_home(&f.data, stale, &f.real).unwrap();
    assert!(live_home.is_dir() && stale_home.is_dir());
    let root = live_home.parent().unwrap();
    let junk = root.join("not-a-task-name");
    std::fs::write(&junk, "junk").unwrap();
    codex::sweep_codex_homes(&f.data, |task| task == live).unwrap();
    assert!(live_home.is_dir());
    assert!(!stale_home.exists());
    assert!(!junk.exists());
    // Removing one task's home is idempotent and never touches another's.
    codex::remove_codex_home(&f.data, live).unwrap();
    assert!(!live_home.exists());
    codex::remove_codex_home(&f.data, live).unwrap();

    // A home whose task no longer exists goes too.
    let gone = codex::prepare_home(
        &f.data,
        "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        &f.real,
    )
    .unwrap();
    codex::sweep_codex_homes(&f.data, |_| false).unwrap();
    assert!(!gone.exists());
}

#[cfg(target_os = "macos")]
#[test]
fn the_profile_closes_the_real_home_and_opens_the_private_one() {
    let f = fixture();
    let configured = rules(&f.data)
        .with_codex_worker_home_at(TASK, &f.real)
        .unwrap();
    let home = configured.codex_home().unwrap().to_path_buf();
    let text = super::macos::profile(&configured);
    assert!(!text.contains(f.real.to_str().unwrap()), "{text}");
    assert!(!text.contains(home.to_str().unwrap()), "{text}");
    assert!(text.contains("param \"C0\""), "{text}");
    assert!(text.contains("(deny file-read* file-write* (subpath (param \"C0\")))"));
    assert!(
        text.contains("(allow file-read* file-write* (subpath (param \"W1\")))"),
        "{text}"
    );
}

#[cfg(target_os = "macos")]
mod sandbox {
    use std::path::Path;
    use std::process::Output;

    use super::{TASK, fixture, rules};
    use crate::host::confine::{Confinement, codex};

    fn run(rules: &Confinement, cwd: &Path, script: &str) -> Output {
        let mut command = crate::host::process::resolve("sh").expect("sh");
        rules.wrap(&mut command).expect("the tool is there");
        std::process::Command::new(&command.program)
            .args(&command.prefix)
            .args(["-c", script])
            .current_dir(cwd)
            .output()
            .expect("starts")
    }

    fn run_proof(rules: &Confinement, cwd: &Path, script: &str) -> Output {
        let mut command = crate::host::process::resolve("sh").expect("sh");
        rules.wrap_proof(&mut command).expect("the proof is there");
        std::process::Command::new(&command.program)
            .args(&command.prefix)
            .args(["-c", script])
            .current_dir(cwd)
            .output()
            .expect("starts")
    }

    #[test]
    fn a_codex_worker_reads_its_own_home_but_not_the_real_or_another_tasks_home() {
        if crate::host::confine::unavailable().is_some() {
            return;
        }
        let f = fixture();
        let configured = rules(&f.data)
            .with_codex_worker_home_at(TASK, &f.real)
            .unwrap();
        let home = configured.codex_home().unwrap().to_path_buf();
        let checkout = std::fs::canonicalize(f.data.join("workspace/repo/p")).unwrap();
        let other = codex::home_path(
            &f.data,
            "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        )
        .unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("secret"), "another task's session").unwrap();

        let own = run(
            &configured,
            &checkout,
            &format!("cat '{}'", home.join("auth.json").display()),
        );
        assert!(own.status.success(), "{own:?}");
        assert!(
            String::from_utf8_lossy(&own.stdout).contains("sk-fake-api-key"),
            "{own:?}"
        );
        for denied in [
            f.real.join("auth.json"),
            other.join("secret"),
            f.data.join("orbyn.sqlite3"),
        ] {
            let out = run(
                &configured,
                &checkout,
                &format!("cat '{}'", denied.display()),
            );
            assert!(!out.status.success(), "{} was readable", denied.display());
            assert!(out.stdout.is_empty(), "{}: {out:?}", denied.display());
        }
        let out = run(
            &configured,
            &checkout,
            &format!(
                "echo mine > '{}/note' && cat '{}/note'",
                home.display(),
                home.display()
            ),
        );
        assert!(out.status.success(), "{out:?}");
        assert_eq!(out.stdout, b"mine\n");
    }

    /// The macOS probe for Codex workers (`docs/crew.md`): the real `codex app-server` starts under
    /// the worker profile with only its private home, answers `initialize`, and the real home stays
    /// closed. Run by hand: `cargo test -p orbyn-core --lib codex_app_server_starts_under_the_profile -- --ignored`.
    #[test]
    #[ignore = "needs the real codex binary on this Mac"]
    fn codex_app_server_starts_under_the_profile_with_its_private_home() {
        use std::io::{BufRead, BufReader, Write};
        use std::process::Stdio;
        if crate::host::confine::unavailable().is_some() {
            return;
        }
        let Ok(codex_program) = crate::host::process::resolve("codex") else {
            return;
        };
        let f = fixture();
        let configured = rules(&f.data)
            .with_codex_worker_home_at(TASK, &f.real)
            .unwrap();
        let home = configured.codex_home().unwrap().to_path_buf();
        let checkout = std::fs::canonicalize(f.data.join("workspace/repo/p")).unwrap();
        let mut command = codex_program;
        command.args = vec!["app-server".into()];
        configured.wrap(&mut command).expect("the tool is there");
        let mut child = std::process::Command::new(&command.program)
            .args(&command.prefix)
            .args(&command.args)
            .env("CODEX_HOME", &home)
            .current_dir(&checkout)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("starts");
        let mut stdin = child.stdin.take().unwrap();
        writeln!(
            stdin,
            r#"{{"id":1,"method":"initialize","params":{{"clientInfo":{{"name":"orbyn-probe","version":"0"}}}}}}"#
        )
        .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = BufReader::new(stdout).read_line(&mut line);
            let _ = sender.send(line);
        });
        let answer = receiver.recv_timeout(std::time::Duration::from_secs(30));
        let _ = child.kill();
        let stderr = {
            let mut text = String::new();
            use std::io::Read;
            let _ = child.stderr.take().unwrap().read_to_string(&mut text);
            text
        };
        let answer = answer.unwrap_or_else(|_| panic!("no answer from codex; stderr: {stderr}"));
        assert!(answer.contains("\"result\""), "{answer} / {stderr}");
        // It may have written its own state in the private home, never in the real one.
        assert!(!f.real.join("state.sqlite3-wal").exists());
        assert_eq!(
            std::fs::read_to_string(f.real.join("history.jsonl")).unwrap(),
            "old prompt"
        );
    }

    #[test]
    fn the_proof_profile_writes_only_in_its_own_run_folder() {
        if crate::host::confine::unavailable().is_some()
            || crate::host::confine::proof_unavailable().is_some()
        {
            return;
        }
        let f = fixture();
        let configured = rules(&f.data)
            .with_codex_worker_home_at(TASK, &f.real)
            .unwrap();
        let checkout = std::fs::canonicalize(f.data.join("workspace/repo/p")).unwrap();
        let run_dir = configured.run_dir.clone();
        let outside = f.base.join("proof-outside");
        for path in [f.data.join("proof-out"), outside.clone()] {
            let out = run_proof(
                &configured,
                &checkout,
                &format!("echo x > '{}'", path.display()),
            );
            assert!(!out.status.success(), "{} was written", path.display());
            assert!(!path.exists(), "{} was written", path.display());
        }
        let out = run_proof(
            &configured,
            &checkout,
            &format!("echo ok > '{}'", run_dir.join("proof-note").display()),
        );
        assert!(out.status.success(), "{out:?}");
        assert_eq!(std::fs::read(run_dir.join("proof-note")).unwrap(), b"ok\n");
    }
}

#[test]
fn an_api_key_login_that_also_holds_chatgpt_tokens_copies_only_the_key() {
    let f = fixture();
    std::fs::write(
        f.real.join("auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-fake-api-key","tokens":{"refresh_token":"rt-fake-chatgpt"}}"#,
    )
    .unwrap();
    let configured = rules(&f.data)
        .with_codex_worker_home_at(TASK, &f.real)
        .unwrap();
    let home = configured.codex_home().unwrap().to_path_buf();
    let text = std::fs::read_to_string(home.join("auth.json")).unwrap();
    assert!(text.contains("sk-fake-api-key"));
    assert!(!text.contains("rt-fake-chatgpt") && !text.contains("tokens"));
    // Nothing in the worker's home, at any depth, may hold anything but an API-key auth.
    let mut stack = vec![home.clone()];
    let mut files = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            files += 1;
            let body = std::fs::read_to_string(&path).unwrap();
            let value: serde_json::Value = serde_json::from_str(&body).unwrap();
            let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
            assert_eq!(keys, ["OPENAI_API_KEY", "auth_mode"], "{}", path.display());
            assert_eq!(value["auth_mode"], "apikey");
        }
    }
    assert_eq!(files, 1);
}
