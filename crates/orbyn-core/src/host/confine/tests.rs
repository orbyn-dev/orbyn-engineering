use std::path::Path;

use orbyn_contracts::EngineTier;

use super::*;

#[test]
fn terminal_engines_and_codex_are_refused_and_others_are_not() {
    assert!(engine_refusal(EngineTier::Terminal, "any").is_some());
    assert_eq!(
        engine_refusal(EngineTier::Native, "codex").is_some(),
        !CODEX_WORKERS_ALLOWED
    );
    assert!(engine_refusal(EngineTier::Native, "claude").is_none());
    assert!(engine_refusal(EngineTier::Acp, "acp-agent").is_none());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn where_no_restriction_exists_nothing_is_wrapped_and_the_reason_is_plain() {
    assert_eq!(unavailable().as_deref(), Some(UNSUPPORTED_PLATFORM));
    let dir = crate::testing::TempDir::new();
    let run = dir.path().join("runs");
    std::fs::create_dir_all(&run).unwrap();
    let rules = Confinement::new(dir.path(), &run, &[], &[]).unwrap();
    let mut command = ResolvedCommand {
        program: "engine".into(),
        prefix: Vec::new(),
        path: "engine".into(),
    };
    assert!(rules.wrap(&mut command).is_err());
    assert_eq!(command.program, Path::new("engine"), "left as it was");
}

#[test]
fn a_path_that_does_not_resolve_is_an_error() {
    let dir = crate::testing::TempDir::new();
    assert!(Confinement::new(&dir.path().join("missing"), dir.path(), &[], &[]).is_err());
    let run = dir.path().join("runs");
    std::fs::create_dir_all(&run).unwrap();
    assert!(Confinement::new(dir.path(), &run, &[&dir.path().join("missing")], &[]).is_err());
    assert!(Confinement::new(dir.path(), &run, &[], &[]).is_ok());
}

#[test]
fn an_allowance_that_contains_the_data_folder_is_refused() {
    let dir = crate::testing::TempDir::new();
    let data = dir.path().join("data");
    let inner = data.join("workspace");
    std::fs::create_dir_all(&inner).unwrap();
    // Equal to, or above, the data folder: refused for every kind of allowance.
    for wide in [data.as_path(), dir.path()] {
        assert!(Confinement::new(&data, &inner, &[wide], &[]).is_err());
        assert!(Confinement::new(&data, &inner, &[], &[wide]).is_err());
        assert!(Confinement::new(&data, wide, &[], &[]).is_err());
    }
    // Inside the data folder, such as the managed workspace, is fine.
    assert!(Confinement::new(&data, &inner, &[&inner], &[&inner]).is_ok());
}

#[test]
fn an_installed_agent_may_run_from_its_own_folder_only() {
    let dir = crate::testing::TempDir::new();
    let agent = dir.path().join("acp-agents/one");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::write(agent.join("bin"), "").unwrap();
    let run = dir.path().join("runs");
    std::fs::create_dir_all(&run).unwrap();
    let rules = Confinement::new(dir.path(), &run, &[], &[]).unwrap();
    assert_eq!(
        rules.for_program(&agent.join("bin")).allow_exec,
        [std::fs::canonicalize(&agent).unwrap()]
    );
    assert!(
        rules
            .for_program(Path::new("/bin/sh"))
            .allow_exec
            .is_empty()
    );
}

#[test]
fn the_env_scrub_drops_the_data_folder_variable() {
    let dir = crate::testing::TempDir::new();
    let run = dir.path().join("runs");
    std::fs::create_dir_all(&run).unwrap();
    let rules = Confinement::new(dir.path(), &run, &[], &[]).unwrap();
    let mut command = Command::new("true");
    command.env("ORBYN_DATA_DIR", "/somewhere");
    rules.scrub_env(&mut command);
    let value = command
        .get_envs()
        .find(|(name, _)| *name == std::ffi::OsStr::new("ORBYN_DATA_DIR"))
        .map(|(_, value)| value.map(ToOwned::to_owned));
    assert_eq!(value, Some(None), "removed, not set");
}

#[cfg(target_os = "macos")]
mod macos_tests {
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::process::Output;

    use super::*;
    use crate::agents::driver::pipes::spawn_engine_confined;

    /// A data folder reached through the `/var` link, with a store, a service handle, the
    /// worker's run folder, another run's folder and a checkout inside the data folder.
    struct Setup {
        _guard: PathBuf,
        data: PathBuf,
        rules: Confinement,
    }

    fn setup() -> Setup {
        // Not canonicalised on purpose: `/var/folders/...` is a link to `/private/var`.
        let base = std::env::temp_dir().join(format!("orbyn-confine-{}", crate::util::new_id()));
        let data = base.join("data");
        for dir in [
            "runs/mine",
            "runs/other",
            "workspace/repo/p",
            "workspace/lib",
        ] {
            std::fs::create_dir_all(data.join(dir)).unwrap();
        }
        for (file, text) in [
            ("orbyn.sqlite3", "store"),
            ("service.json", "{}"),
            ("runs/mine/mcp.json", "mine"),
            ("runs/other/mcp.json", "other"),
            ("workspace/lib/skill.md", "skill"),
        ] {
            std::fs::write(data.join(file), text).unwrap();
        }
        let rules = Confinement::new(
            &data,
            &data.join("runs/mine"),
            &[&data.join("workspace/lib")],
            &[&data.join("workspace/repo/p")],
        )
        .unwrap();
        Setup {
            _guard: base,
            data,
            rules,
        }
    }

    impl Drop for Setup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self._guard);
        }
    }

    fn run(rules: &Confinement, cwd: &Path, script: &str) -> Output {
        let mut command = crate::host::process::resolve("sh").expect("sh");
        rules.wrap(&mut command).expect("the tool is there");
        let mut builder = std::process::Command::new(&command.program);
        builder
            .args(&command.prefix)
            .args(["-c", script])
            .current_dir(cwd);
        builder.output().expect("starts")
    }

    #[test]
    fn the_tool_is_there_and_not_inside_another_restriction() {
        assert_eq!(unavailable(), None);
    }

    #[test]
    fn a_missing_tool_and_a_tool_that_refuses_are_reported() {
        let missing = macos::check(Path::new("/nonexistent/sandbox-exec"));
        assert!(missing.unwrap().contains("missing"));
        // `false` stands in for a tool that cannot start a profile, as in a process that
        // already runs under one (exit 71 there).
        let refused = macos::check(Path::new("/usr/bin/false"));
        assert!(refused.unwrap().contains("restriction"));
    }

    #[test]
    fn the_profile_names_no_path() {
        let s = setup();
        let text = macos::profile(&s.rules);
        assert!(!text.contains(s.data.to_str().unwrap()));
        assert!(!text.contains("/private"));
        assert!(text.contains("param \"W0\""));
    }

    #[test]
    fn the_store_and_the_service_handle_cannot_be_read_but_the_run_folder_can() {
        let s = setup();
        let data = std::fs::canonicalize(&s.data).unwrap();
        let cwd = data.join("workspace/repo/p");
        for denied in ["orbyn.sqlite3", "service.json", "runs/other/mcp.json"] {
            let out = run(
                &s.rules,
                &cwd,
                &format!("cat '{}'", data.join(denied).display()),
            );
            assert!(!out.status.success(), "{denied} was readable");
            assert!(out.stdout.is_empty(), "{denied}");
        }
        // The same path through the `/var` link is denied too.
        let linked = s.data.join("orbyn.sqlite3");
        let out = run(&s.rules, &cwd, &format!("cat '{}'", linked.display()));
        assert!(!out.status.success() && out.stdout.is_empty());
        // Listing the data folder, and writing into it, fail.
        let out = run(&s.rules, &cwd, &format!("ls '{}'", data.display()));
        assert!(!out.status.success());
        let out = run(
            &s.rules,
            &cwd,
            &format!("echo x > '{}/new'", data.display()),
        );
        assert!(!out.status.success());
        assert!(!data.join("new").exists());
    }

    #[test]
    fn the_later_allowance_beats_the_earlier_denial() {
        let s = setup();
        let data = std::fs::canonicalize(&s.data).unwrap();
        let cwd = data.join("workspace/repo/p");
        let out = run(
            &s.rules,
            &cwd,
            &format!("cat '{}'", data.join("runs/mine/mcp.json").display()),
        );
        assert!(out.status.success(), "{out:?}");
        assert_eq!(out.stdout, b"mine");
        let out = run(
            &s.rules,
            &cwd,
            &format!("echo ok > '{}/runs/mine/note'", data.display()),
        );
        assert!(out.status.success(), "{out:?}");
        // The library is read-only, the checkout read-write.
        let out = run(
            &s.rules,
            &cwd,
            &format!("cat '{}'", data.join("workspace/lib/skill.md").display()),
        );
        assert_eq!(out.stdout, b"skill");
        let out = run(
            &s.rules,
            &cwd,
            &format!("echo x > '{}/workspace/lib/new'", data.display()),
        );
        assert!(!out.status.success());
        let out = run(&s.rules, &cwd, "echo work > f && cat f && pwd");
        assert!(out.status.success(), "{out:?}");
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("work\n"));
    }

    #[test]
    fn the_rest_of_the_person_s_files_still_read() {
        let s = setup();
        let home = std::env::var("HOME").unwrap();
        let out = run(
            &s.rules,
            Path::new(&home),
            "ls . >/dev/null && cat /etc/hosts >/dev/null",
        );
        assert!(out.status.success(), "{out:?}");
    }

    #[tokio::test]
    async fn the_spawn_path_restricts_a_piped_engine_and_drops_the_data_folder_variable() {
        let s = setup();
        let data = std::fs::canonicalize(&s.data).unwrap();
        let sh = crate::host::process::resolve("sh").unwrap();
        let script = format!(
            "test -z \"$ORBYN_DATA_DIR\" && cat '{}' || echo denied",
            data.join("orbyn.sqlite3").display()
        );
        let args = [OsString::from("-c"), OsString::from(script)];
        // SAFETY of the test: the variable is only read by the spawned child.
        let process = spawn_engine_confined(
            &sh,
            &args,
            &data.join("workspace/repo/p"),
            &[],
            Some(&s.rules),
        );
        let mut process = process.expect("starts under the restriction");
        let mut out = String::new();
        tokio::io::AsyncReadExt::read_to_string(&mut process.stdout, &mut out)
            .await
            .unwrap();
        process.child.wait().await.unwrap();
        assert_eq!(out.trim(), "denied");
    }

    /// A program the worker wrote, started through LaunchServices, would run from launchd
    /// outside the profile and could copy the service handle into the run folder.
    #[test]
    fn a_program_the_worker_wrote_cannot_be_started_through_launchservices() {
        let s = setup();
        let data = std::fs::canonicalize(&s.data).unwrap();
        let cwd = data.join("workspace/repo/p");
        let app = data.join("runs/mine/escape.app");
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(
            app.join("Contents/Info.plist"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict>\
             <key>CFBundleExecutable</key><string>run</string>\
             <key>CFBundleIdentifier</key><string>dev.orbyn.confine-test</string>\
             <key>CFBundlePackageType</key><string>APPL</string>\
             <key>LSUIElement</key><true/></dict></plist>",
        )
        .unwrap();
        let out = data.join("runs/mine/stolen");
        let script = format!(
            "#!/bin/sh\ncat '{}' > '{}'\n",
            data.join("service.json").display(),
            out.display()
        );
        let exe = app.join("Contents/MacOS/run");
        std::fs::write(&exe, script).unwrap();
        std::fs::set_permissions(&exe, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let opened = run(
            &s.rules,
            &cwd,
            &format!("/usr/bin/open -g -j '{}'", app.display()),
        );
        assert!(!opened.status.success(), "open started it: {opened:?}");
        std::thread::sleep(std::time::Duration::from_secs(3));
        assert!(!out.exists(), "the program ran outside the profile");
    }

    #[test]
    fn a_worker_cannot_signal_another_process_but_can_signal_its_own_children() {
        let s = setup();
        let data = std::fs::canonicalize(&s.data).unwrap();
        let cwd = data.join("workspace/repo/p");
        let mut other = std::process::Command::new("sleep")
            .arg("60")
            .spawn()
            .unwrap();
        let out = run(&s.rules, &cwd, &format!("kill -TERM {}", other.id()));
        assert!(!out.status.success(), "{out:?}");
        assert!(
            other.try_wait().unwrap().is_none(),
            "the other process was signalled"
        );
        other.kill().unwrap();
        let _ = other.wait();
        let own = run(&s.rules, &cwd, "sleep 60 & kill -TERM $!");
        assert!(own.status.success(), "{own:?}");
    }
}
