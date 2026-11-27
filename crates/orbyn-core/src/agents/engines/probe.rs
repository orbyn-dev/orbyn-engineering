//! Read-only engine probes. They run version and login status commands from a
//! neutral directory, so no repository config or hooks load, and open no session.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use orbyn_contracts::{AuthState, BinaryProbe};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::super::driver::EngineProbe;
use crate::host::process::{ResolvedCommand, resolve};

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const BINARY_TIMEOUT: Duration = Duration::from_secs(5);
/// A status command has no reason to print more than this.
const MAX_CAPTURE: u64 = 256 * 1024;

pub(crate) struct Captured {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Runs a short command and captures its output, or `None` on spawn failure or timeout.
pub(crate) async fn capture(
    command: &ResolvedCommand,
    args: &[&str],
    timeout: Duration,
) -> Option<Captured> {
    capture_with(command, args, timeout, false).await
}

/// What a program of unknown origin may see: where to find things, a home, a locale,
/// and on Windows the few variables it cannot start without. No login variable.
fn minimal_env(builder: &mut std::process::Command) {
    builder.env_clear();
    for (name, value) in std::env::vars_os() {
        let key = name.to_string_lossy().to_ascii_uppercase();
        let keep = matches!(key.as_str(), "PATH" | "HOME" | "LANG")
            || key.starts_with("LC_")
            || (cfg!(windows)
                && matches!(
                    key.as_str(),
                    "SYSTEMROOT" | "TEMP" | "TMP" | "PATHEXT" | "USERPROFILE"
                ));
        if keep {
            builder.env(name, value);
        }
    }
}

/// `capture`, and with `minimal` the child gets only `minimal_env` rather than the
/// environment engines inherit.
async fn capture_with(
    command: &ResolvedCommand,
    args: &[&str],
    timeout: Duration,
    minimal: bool,
) -> Option<Captured> {
    let mut builder = Command::new(&command.program);
    builder
        .args(&command.prefix)
        .args(args)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if minimal {
        minimal_env(builder.as_std_mut());
    } else {
        crate::host::child_env::for_engine(builder.as_std_mut());
    }
    let mut child = builder.spawn().ok()?;
    let (stdout, stderr) = (child.stdout.take()?, child.stderr.take()?);
    let read = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (mut limited_out, mut limited_err) =
            (stdout.take(MAX_CAPTURE), stderr.take(MAX_CAPTURE));
        let (first, second) = tokio::join!(
            limited_out.read_to_end(&mut out),
            limited_err.read_to_end(&mut err)
        );
        first.ok()?;
        second.ok()?;
        // Past the limit the rest goes unread, so this ends it rather than waiting.
        let status = match child.try_wait().ok()? {
            Some(status) => status,
            None if out.len() as u64 >= MAX_CAPTURE || err.len() as u64 >= MAX_CAPTURE => {
                child.start_kill().ok()?;
                child.wait().await.ok()?
            }
            None => child.wait().await.ok()?,
        };
        Some((status, out, err))
    };
    let (status, out, err) = tokio::time::timeout(timeout, read).await.ok()??;
    Some(Captured {
        status: status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr: String::from_utf8_lossy(&err).into_owned(),
    })
}

pub(crate) async fn probe_cli(program: &str, version_args: &[&str]) -> EngineProbe {
    let Some(command) = resolve(program) else {
        return EngineProbe::default();
    };
    let mut probe = EngineProbe {
        path: Some(command.path.clone()),
        ..EngineProbe::default()
    };
    match capture(&command, version_args, PROBE_TIMEOUT).await {
        Some(output) if output.status == 0 => {
            probe.version = parse_version(&output.stdout);
            if probe.version.is_none() {
                probe.problem = Some("Could not read this engine's version.".into());
            }
        }
        Some(_) => probe.problem = Some("This engine's version command failed.".into()),
        None => probe.problem = Some("This engine did not respond.".into()),
    }
    probe
}

/// The login state and, when the CLI's status names one, the account.
pub(crate) async fn read_login(
    program: &str,
    args: &[&str],
    parse: fn(i32, &str) -> AuthState,
    account: fn(&str) -> Option<String>,
) -> (AuthState, Option<String>) {
    let Some(command) = resolve(program) else {
        return (AuthState::Unknown, None);
    };
    match capture(&command, args, PROBE_TIMEOUT).await {
        // Some CLIs print their login line on stderr.
        Some(output) => {
            let text = format!("{}{}", output.stdout, output.stderr);
            (parse(output.status, &text), account(&text))
        }
        None => (AuthState::Unknown, None),
    }
}

/// Runs `--version` on an absolute path and reports what it printed. No shell is
/// involved, the argument is fixed, and the run ends after a few seconds.
pub async fn probe_binary(path: &str) -> BinaryProbe {
    let missing = BinaryProbe {
        exists: false,
        path: None,
        version: None,
        problem: None,
    };
    if !Path::new(path).is_absolute() {
        return BinaryProbe {
            problem: Some("Give the full path to the program.".into()),
            ..missing
        };
    }
    let Some(command) = resolve(path) else {
        return missing;
    };
    let mut found = BinaryProbe {
        exists: true,
        path: Some(command.path.to_string_lossy().into_owned()),
        ..missing
    };
    match capture_with(&command, &["--version"], BINARY_TIMEOUT, true).await {
        Some(output) if output.status == 0 => {
            found.version = parse_version(&output.stdout);
            if found.version.is_none() {
                found.problem = Some("It ran, but printed no version.".into());
            }
        }
        Some(_) => found.problem = Some("Its version command failed.".into()),
        None => found.problem = Some("It did not start or did not answer in time.".into()),
    }
    found
}

/// The first dotted number in a version line, such as `2.1.150 (Claude Code)`
/// or `codex-cli 0.166.0`.
pub fn parse_version(text: &str) -> Option<String> {
    text.split_whitespace().find_map(|word| {
        let word = word.trim_start_matches('v');
        let dotted = word.split('.').count() >= 2
            && word.split('.').all(|part| {
                !part.is_empty() && part.chars().next().is_some_and(|c| c.is_ascii_digit())
            });
        dotted.then(|| word.to_owned())
    })
}

/// The symlink-free path used for ownership checks.
pub(crate) fn real_path(path: &Path) -> PathBuf {
    crate::util::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_real_version_lines() {
        assert_eq!(
            parse_version("2.1.150 (Claude Code)\n").as_deref(),
            Some("2.1.150")
        );
        assert_eq!(
            parse_version("codex-cli 0.166.0\n").as_deref(),
            Some("0.166.0")
        );
        assert_eq!(parse_version("no version here"), None);
    }

    #[test]
    fn a_program_of_unknown_origin_gets_only_the_plain_environment() {
        let mut command = std::process::Command::new("x");
        minimal_env(&mut command);
        let names: Vec<String> = command
            .get_envs()
            .map(|(name, _)| name.to_string_lossy().to_ascii_uppercase())
            .collect();
        assert!(names.iter().any(|name| name == "PATH"));
        assert!(names.iter().all(|name| {
            matches!(
                name.as_str(),
                "PATH"
                    | "HOME"
                    | "LANG"
                    | "SYSTEMROOT"
                    | "TEMP"
                    | "TMP"
                    | "PATHEXT"
                    | "USERPROFILE"
            ) || name.starts_with("LC_")
        }));
        // Cargo sets this for the test, and a login variable would travel the same way.
        assert!(!names.iter().any(|name| name == "CARGO"));
    }

    #[tokio::test]
    async fn a_program_path_reports_its_version() {
        // Cargo sets CARGO for the tests it runs, and `cargo --version` is a real program.
        let cargo = std::env::var("CARGO").expect("run by cargo");
        let found = probe_binary(&cargo).await;
        assert!(found.exists, "{found:?}");
        assert!(found.version.is_some(), "{found:?}");
        assert!(found.problem.is_none());
    }

    #[tokio::test]
    async fn a_path_that_is_missing_relative_or_a_folder_is_not_run() {
        let missing = probe_binary(
            &std::env::temp_dir()
                .join("orbyn-no-such-program")
                .to_string_lossy(),
        )
        .await;
        assert!(!missing.exists && missing.problem.is_none());
        let folder = probe_binary(&std::env::temp_dir().to_string_lossy()).await;
        assert!(!folder.exists);
        let relative = probe_binary("cargo").await;
        assert!(!relative.exists && relative.problem.is_some());
    }

    /// Writes the shell script a capture test runs, on the platforms a shell is there.
    #[cfg(unix)]
    fn status_script(dir: &Path, name: &str, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    /// Runs a status script through the capture the login probe uses.
    #[cfg(unix)]
    async fn capture_status(script: &str) -> Option<Captured> {
        let command = resolve(script).expect("the status script resolves");
        capture(&command, &[], Duration::from_secs(10)).await
    }

    /// I56's size bound, through the capture the login probe uses: below the 256 KiB the
    /// architecture names every byte arrives with the exit status, at it the whole answer
    /// arrives, past it exactly the first 256 KiB per stream is kept and the rest is refused
    /// unread, and a program that keeps writing is ended rather than waited for. The login
    /// probe's mapping from a truncated capture is `the_login_probe_keeps_answering_past_the_capture_cap`.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_status_capture_stops_at_256_kib_and_ends_a_program_that_keeps_writing() {
        assert_eq!(MAX_CAPTURE, 256 * 1024, "the row's 256 KiB cap");
        let cap = MAX_CAPTURE as usize;
        let status_json = r#"{"loggedIn":true,"email":"probe@example.test"}"#;
        let dir = crate::testing::TempDir::new();

        // Under the cap: every byte arrives, and so does the exit status.
        let under = status_script(
            dir.path(),
            "under",
            "printf 'a short status answer'; printf 'a short login note' >&2",
        );
        let output = capture_status(&under)
            .await
            .expect("the status command under the cap answers");
        assert_eq!(output.status, 0, "the exit status is kept under the cap");
        assert_eq!(output.stdout, "a short status answer");
        assert_eq!(output.stderr, "a short login note");

        // Exactly at the cap: the whole answer arrives.
        let at = status_script(dir.path(), "at", "perl -e 'print \"a\" x 262144'");
        let output = capture_status(&at)
            .await
            .expect("the status command at the cap answers");
        assert_eq!(output.stdout.len(), cap, "the last byte at the cap is kept");
        assert!(
            output.stdout.bytes().all(|byte| byte == b'a'),
            "no byte of the answer is replaced"
        );

        // stdout past the cap: exactly the first 256 KiB is kept, and the tail is refused. Its
        // unused stderr is closed first, so the capture's other read reaches EOF and a small
        // pipe cannot block the writer before the cap is reached.
        let over = status_script(
            dir.path(),
            "over",
            concat!(
                "exec 2>/dev/null\n",
                r#"printf '%s' '{"loggedIn":true,"email":"probe@example.test"}'"#,
                "\n",
                r#"perl -e 'print " " x 300000'"#,
                "\n",
                r#"printf 'REFUSED-TAIL'"#,
            ),
        );
        let output = capture_status(&over)
            .await
            .expect("the status command past the cap answers");
        assert_eq!(output.stdout.len(), cap, "only the cap is kept");
        assert!(
            output.stdout.starts_with(status_json),
            "the kept text starts with the answer"
        );
        assert!(
            output.stdout[status_json.len()..]
                .bytes()
                .all(|byte| byte == b' '),
            "the kept text is the first cap bytes"
        );
        assert!(
            !output.stdout.contains("REFUSED-TAIL"),
            "the tail past the cap is refused"
        );

        // stderr past the cap: the same bound, separately, while stdout is closed first so
        // its read reaches EOF and a small pipe cannot block the writer before the cap.
        let stderr_over = status_script(
            dir.path(),
            "stderr-over",
            concat!(
                "exec 1>/dev/null\n",
                r#"printf '%s' '{"loggedIn":true,"email":"probe@example.test"}' >&2"#,
                "\n",
                r#"perl -e 'print " " x 300000; print "REFUSED-TAIL"' >&2"#,
            ),
        );
        let output = capture_status(&stderr_over)
            .await
            .expect("the status command past the cap on stderr answers");
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr.len(), cap, "only the cap is kept from stderr");
        assert!(output.stderr.starts_with(status_json));
        assert!(
            output.stderr[status_json.len()..]
                .bytes()
                .all(|byte| byte == b' '),
            "the kept stderr is the first cap bytes"
        );
        assert!(
            !output.stderr.contains("REFUSED-TAIL"),
            "the tail past the cap on stderr is refused"
        );

        // A program that keeps writing past the cap is ended, not waited for: if it were
        // waited for it would sleep 60 s and then leave this marker. Its stderr is closed so
        // the capture's other read reaches EOF and the cap alone decides to end it. The pid
        // is checked directly, because the marker path goes with the test folder.
        let marker = dir.path().join("kept-writing");
        let pid_file = dir.path().join("kept-writing.pid");
        let endless = status_script(
            dir.path(),
            "endless",
            &format!(
                "exec 2>/dev/null\nprintf '%s' $$ > '{}'\nperl -e 'print \"b\" x 262145'\nsleep 60\nprintf survived > '{}'",
                pid_file.display(),
                marker.display()
            ),
        );
        let started = std::time::Instant::now();
        let output = capture_status(&endless)
            .await
            .expect("a program past the cap is ended, not waited for");
        assert_eq!(output.stdout.len(), cap, "only the cap is kept");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the cap ended the program at once"
        );
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .expect("the program wrote its pid before writing past the cap")
            .trim()
            .parse()
            .expect("a pid");
        crate::testing::assert_process_reaped(pid, "a program that wrote past the capture cap")
            .await;
        assert!(
            !marker.exists(),
            "the program never reached its line after the write"
        );
    }
}
