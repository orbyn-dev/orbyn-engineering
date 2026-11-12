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

/// A Claude folder with the shapes of the real one: a lead's transcript and a worker's own,
/// another project's, the auto-memory folder and the prompt history.
fn claude_folder(base: &Path) -> PathBuf {
    // Characters a pattern would read as operators, and a space, on purpose.
    let config = base.join("claude cfg (1)+x.y");
    for dir in [
        "projects/-work-app/own/subagents",
        "projects/-work-app/memory",
        "projects/-work-other",
    ] {
        std::fs::create_dir_all(config.join(dir)).unwrap();
    }
    for (file, text) in [
        ("projects/-work-app/lead.jsonl", "lead thread text"),
        ("projects/-work-app/own.jsonl", "own"),
        ("projects/-work-app/own/subagents/a.jsonl", "sub"),
        ("projects/-work-app/memory/MEMORY.md", "memory"),
        ("projects/-work-other/x.jsonl", "other project"),
        ("history.jsonl", "typed prompts"),
        (".claude.json", "login state"),
    ] {
        std::fs::write(config.join(file), text).unwrap();
    }
    config
}

#[test]
fn transcript_rules_fail_closed_when_the_folder_or_the_id_is_unusable() {
    let dir = crate::testing::TempDir::new();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("runs/mine")).unwrap();
    let rules = Confinement::new(&data, &data.join("runs/mine"), &[], &[]).unwrap();
    let config = claude_folder(dir.path());
    let projects = config.join("projects");
    assert!(
        rules
            .clone()
            .with_claude_transcripts(&projects, Some("own"), Path::new("/"))
            .is_ok()
    );
    // The configuration folder does not resolve, or is relative.
    let missing = dir.path().join("nowhere/projects");
    assert!(
        rules
            .clone()
            .with_claude_transcripts(&missing, Some("own"), Path::new("/"))
            .is_err()
    );
    assert!(
        rules
            .clone()
            .with_claude_transcripts(Path::new("projects"), None, Path::new("/"))
            .is_err()
    );
    // An id that could change the pattern, or is empty, is refused.
    for bad in ["", "a/b", "a.b", "a)b", "../x", "a b", "a*"] {
        assert!(
            rules
                .clone()
                .with_claude_transcripts(&projects, Some(bad), Path::new("/"))
                .is_err(),
            "{bad:?}"
        );
    }
    // Folders that overlap the data folder in either direction are refused.
    let inside = data.join("claude/projects");
    std::fs::create_dir_all(&inside).unwrap();
    assert!(
        rules
            .clone()
            .with_claude_transcripts(&inside, Some("own"), Path::new("/"))
            .is_err()
    );
    let outer = dir.path().join("c2/projects");
    let inner = outer.join("data");
    std::fs::create_dir_all(inner.join("run")).unwrap();
    let above = Confinement::new(&inner, &inner.join("run"), &[], &[]).unwrap();
    assert!(
        above
            .with_claude_transcripts(&outer, Some("own"), Path::new("/"))
            .is_err()
    );
}

/// A link is not followed by a rule by path and could be swapped later, so nothing starts.
#[cfg(unix)]
#[test]
fn a_linked_projects_folder_or_history_file_refuses_the_worker() {
    let dir = crate::testing::TempDir::new();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("runs/mine")).unwrap();
    let rules = Confinement::new(&data, &data.join("runs/mine"), &[], &[]).unwrap();
    let config = claude_folder(dir.path());
    let elsewhere = dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("hist"), "h").unwrap();
    // A linked history file.
    std::fs::remove_file(config.join("history.jsonl")).unwrap();
    std::os::unix::fs::symlink(elsewhere.join("hist"), config.join("history.jsonl")).unwrap();
    let projects = config.join("projects");
    assert!(
        rules
            .clone()
            .with_claude_transcripts(&projects, Some("own"), Path::new("/"))
            .is_err()
    );
    assert!(rules.clone().with_closed_claude_folder(&projects).is_err());
    std::fs::remove_file(config.join("history.jsonl")).unwrap();
    assert!(
        rules
            .clone()
            .with_claude_transcripts(&projects, Some("own"), Path::new("/"))
            .is_ok()
    );
    // A linked projects folder.
    std::fs::rename(&projects, dir.path().join("real-projects")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("real-projects"), &projects).unwrap();
    assert!(
        rules
            .clone()
            .with_claude_transcripts(&projects, Some("own"), Path::new("/"))
            .is_err()
    );
    assert!(rules.with_closed_claude_folder(&projects).is_err());
}

/// A link in front of the configuration folder, or above it, could be swapped for another
/// folder that the rules do not name: the worker is refused. So is a start folder whose
/// Claude name cannot be told with confidence.
#[cfg(unix)]
#[test]
fn a_linked_configuration_folder_or_an_uncertain_start_folder_refuses_the_worker() {
    let dir = crate::testing::TempDir::new();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("runs/mine")).unwrap();
    let rules = Confinement::new(&data, &data.join("runs/mine"), &[], &[]).unwrap();
    let real = claude_folder(dir.path());
    let linked = dir.path().join("linked-config");
    std::os::unix::fs::symlink(&real, &linked).unwrap();
    for config in [linked.clone(), linked.join("nested")] {
        let projects = config.join("projects");
        assert!(
            rules
                .clone()
                .with_claude_transcripts(&projects, Some("own"), Path::new("/"))
                .is_err()
        );
        assert!(rules.clone().with_closed_claude_folder(&projects).is_err());
    }
    let projects = real.join("projects");
    assert!(
        rules
            .clone()
            .with_claude_transcripts(&projects, Some("own"), Path::new("/"))
            .is_ok()
    );
    // A long start folder (Claude shortens the name) and a non-ASCII one are refused.
    let long = dir.path().join("x".repeat(210));
    std::fs::create_dir_all(&long).unwrap();
    assert!(
        rules
            .clone()
            .with_claude_transcripts(&projects, Some("own"), &long)
            .is_err()
    );
    let accent = dir.path().join("caf\u{e9}");
    std::fs::create_dir_all(&accent).unwrap();
    assert!(
        rules
            .clone()
            .with_claude_transcripts(&projects, Some("own"), &accent)
            .is_err()
    );
    // A start folder that does not exist is refused too.
    assert!(
        rules
            .with_claude_transcripts(&projects, Some("own"), &dir.path().join("nope"))
            .is_err()
    );
}

#[test]
fn a_first_turn_gets_a_new_uuid_and_a_later_turn_keeps_its_session() {
    let dir = crate::testing::TempDir::new();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("runs/mine")).unwrap();
    let rules = Confinement::new(&data, &data.join("runs/mine"), &[], &[]).unwrap();
    let projects = claude_folder(dir.path()).join("projects");
    let first = rules
        .clone()
        .with_claude_transcripts(&projects, None, Path::new("/"))
        .unwrap();
    let id = first
        .fresh_session()
        .expect("a first turn names its session");
    let parts: Vec<usize> = id.split('-').map(str::len).collect();
    assert_eq!(parts, [8, 4, 4, 4, 12], "{id}");
    assert!(id.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
    assert_eq!(&id[14..15], "4", "version 4");
    let second = rules
        .clone()
        .with_claude_transcripts(&projects, None, Path::new("/"))
        .unwrap();
    assert_ne!(second.fresh_session(), Some(id), "not reused");
    let later = rules
        .with_claude_transcripts(&projects, Some("own"), Path::new("/"))
        .unwrap();
    assert_eq!(
        later.fresh_session(),
        None,
        "a resumed session keeps its id"
    );
    assert_eq!(later.transcripts.unwrap().session.as_deref(), Some("own"));
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

    /// Renaming the data folder or a folder above it would leave its files under a name no
    /// path rule covers. Only the folders inside the test's own temp folder are tried.
    #[test]
    fn the_data_folder_and_every_folder_above_it_cannot_be_renamed() {
        let base = std::env::temp_dir().join(format!("orbyn-confine-up-{}", crate::util::new_id()));
        let data = base.join("home/Library/App Support/Orbyn");
        std::fs::create_dir_all(data.join("runs/mine")).unwrap();
        std::fs::create_dir_all(base.join("home/unrelated")).unwrap();
        std::fs::write(data.join("service.json"), "secret").unwrap();
        let rules = Confinement::new(&data, &data.join("runs/mine"), &[], &[]).unwrap();
        let base_real = std::fs::canonicalize(&base).unwrap();
        let data_real = std::fs::canonicalize(&data).unwrap();
        let cwd = data_real.join("runs/mine");
        // Every folder from the data folder up to `/` has a rule; none holds a path in the text.
        assert_eq!(macos::above_len(&rules), data_real.ancestors().count());
        for ancestor in data_real
            .ancestors()
            .take_while(|a| a.starts_with(&base_real))
        {
            let out = run(
                &rules,
                &cwd,
                &format!("mv '{0}' '{0}-moved'", ancestor.display()),
            );
            assert!(!out.status.success(), "{} was renamed", ancestor.display());
            assert!(ancestor.exists());
        }
        let out = run(
            &rules,
            &cwd,
            &format!("cat '{}/service.json'", data_real.display()),
        );
        assert!(!out.status.success() && out.stdout.is_empty());
        // An unrelated sibling folder still moves.
        let out = run(
            &rules,
            &cwd,
            &format!(
                "mv '{0}/home/unrelated' '{0}/home/moved'",
                base_real.display()
            ),
        );
        assert!(out.status.success(), "{out:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The rules for a worker whose Claude folder is `config`, run in the data folder's checkout.
    fn claude_worker(s: &Setup, config: &Path, session: Option<&str>) -> Confinement {
        s.rules
            .clone()
            .with_claude_transcripts(&config.join("projects"), session, &checkout(s))
            .expect("the folder resolves")
    }

    /// The folder the worker's engine starts in: its checkout.
    fn checkout(s: &Setup) -> PathBuf {
        std::fs::canonicalize(&s.data)
            .unwrap()
            .join("workspace/repo/p")
    }

    /// Claude's project folder for that start folder, which the core makes before launch.
    fn project_of(s: &Setup, config: &Path) -> PathBuf {
        let name =
            crate::agents::claude_history::project_folder_name(&checkout(s).to_string_lossy());
        config.join("projects").join(name)
    }

    fn base_of(s: &Setup) -> PathBuf {
        s._guard.clone()
    }

    #[test]
    fn the_lead_s_transcript_cannot_be_read_or_changed_but_the_workers_own_session_can() {
        let s = setup();
        let config = std::fs::canonicalize(claude_folder(&base_of(&s))).unwrap();
        let rules = claude_worker(&s, &config, Some("own"));
        let cwd = std::fs::canonicalize(&s.data)
            .unwrap()
            .join("workspace/repo/p");
        let app = config.join("projects/-work-app");
        for denied in ["lead.jsonl", "../-work-other/x.jsonl", "memory/MEMORY.md"] {
            let out = run(
                &rules,
                &cwd,
                &format!("cat '{}'", app.join(denied).display()),
            );
            assert!(!out.status.success(), "{denied} was readable");
            assert!(out.stdout.is_empty(), "{denied}");
        }
        // Not listed either: the other session ids are not shown.
        for folder in [app.clone(), config.join("projects")] {
            let out = run(&rules, &cwd, &format!("ls '{}'", folder.display()));
            assert!(!out.status.success(), "{} was listed", folder.display());
        }
        let out = run(
            &rules,
            &cwd,
            &format!("echo x >> '{}'", app.join("lead.jsonl").display()),
        );
        assert!(!out.status.success());
        assert_eq!(
            std::fs::read(app.join("lead.jsonl")).unwrap(),
            b"lead thread text"
        );
        // The prompt history is denied, the rest of the configuration folder is not.
        let out = run(
            &rules,
            &cwd,
            &format!("cat '{}'", config.join("history.jsonl").display()),
        );
        assert!(!out.status.success() && out.stdout.is_empty());
        let out = run(
            &rules,
            &cwd,
            &format!("cat '{}'", config.join(".claude.json").display()),
        );
        assert_eq!(out.stdout, b"login state");
        // Its own session: read, append, a subagent file, a new file under its folder.
        let own = app.join("own.jsonl");
        let out = run(
            &rules,
            &cwd,
            &format!(
                "cat '{o}' && echo more >> '{o}' && cat '{a}/own/subagents/a.jsonl' \
                 && mkdir -p '{a}/own/tool-results' && echo r > '{a}/own/tool-results/1'",
                o = own.display(),
                a = app.display()
            ),
        );
        assert!(out.status.success(), "{out:?}");
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("ownsub"));
        assert_eq!(std::fs::read(&own).unwrap(), b"ownmore\n");
    }

    /// The rule is on the folder, so a transcript that appears after the worker started, such
    /// as the lead's next session, is denied too.
    #[test]
    fn a_transcript_created_after_the_worker_started_is_denied() {
        let s = setup();
        let config = std::fs::canonicalize(claude_folder(&base_of(&s))).unwrap();
        let rules = claude_worker(&s, &config, Some("own"));
        let cwd = std::fs::canonicalize(&s.data)
            .unwrap()
            .join("workspace/repo/p");
        let app = config.join("projects/-work-app");
        let flag = config.join("flag");
        let late = app.join("lead-next.jsonl");
        let script = format!(
            "i=0; while [ ! -f '{f}' ] && [ $i -lt 100 ]; do sleep 0.1; i=$((i+1)); done; \
             cat '{l}' && echo READ",
            f = flag.display(),
            l = late.display()
        );
        let mut command = crate::host::process::resolve("sh").unwrap();
        rules.wrap(&mut command).unwrap();
        let child = std::process::Command::new(&command.program)
            .args(&command.prefix)
            .args(["-c", &script])
            .current_dir(&cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::fs::write(&late, "the lead's next thread").unwrap();
        std::fs::write(&flag, "go").unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(!out.status.success(), "{out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
    }

    /// A first turn has no session file: Claude makes it under the id the rules chose, in the
    /// project folder the core made, and cannot make any other folder or reach a lead's
    /// transcript beside it.
    #[test]
    fn a_first_turn_creates_its_own_session_in_the_prepared_folder_and_nothing_else() {
        let s = setup();
        let base = base_of(&s);
        let config = base.join("fresh");
        std::fs::create_dir_all(&config).unwrap();
        let config = std::fs::canonicalize(&config).unwrap();
        let rules = claude_worker(&s, &config, None);
        let id = rules.fresh_session().unwrap().to_owned();
        let cwd = checkout(&s);
        // `projects` and the project folder were made, private to the person.
        let folder = project_of(&s, &config);
        assert!(folder.is_dir());
        use std::os::unix::fs::PermissionsExt;
        for dir in [config.join("projects"), folder.clone()] {
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "{dir:?} {mode:o}");
        }
        let out = run(
            &rules,
            &cwd,
            &format!(
                "echo one > '{f}/{id}.jsonl' && mkdir '{f}/{id}' && echo two > '{f}/{id}/x' \
                 && mkdir '{f}/{id}/subagents' && echo s > '{f}/{id}/subagents/a.jsonl' \
                 && cat '{f}/{id}.jsonl'",
                f = folder.display()
            ),
        );
        assert!(out.status.success(), "{out:?}");
        assert_eq!(out.stdout, b"one\n");
        // No other folder, file or session may be created, and another thread's is unreadable.
        std::fs::write(folder.join("lead.jsonl"), "lead").unwrap();
        for script in [
            format!("cat '{}/lead.jsonl'", folder.display()),
            format!("echo x > '{}/someone-else.jsonl'", folder.display()),
            format!("mkdir '{}/projects/-other'", config.display()),
            format!("mkdir '{}/-other'", config.join("projects").display()),
            format!("mkdir '{}/other'", folder.display()),
        ] {
            let out = run(&rules, &cwd, &script);
            assert!(
                !out.status.success() && out.stdout.is_empty(),
                "{script}: {out:?}"
            );
        }
    }

    /// The lead's file cannot be reached by moving a folder above it, by linking to it or by
    /// copying it.
    #[test]
    fn the_transcript_folder_cannot_be_reached_by_a_rename_or_a_link() {
        let s = setup();
        let base = base_of(&s);
        let config = std::fs::canonicalize(claude_folder(&base)).unwrap();
        let real = config.join("projects");
        let rules = claude_worker(&s, &config, Some("own"));
        let cwd = std::fs::canonicalize(&s.data)
            .unwrap()
            .join("workspace/repo/p");
        let lead = real.join("-work-app/lead.jsonl");
        let mine = real.join("-work-app/own.jsonl");
        for script in [
            format!("mv '{}' '{}-moved'", config.display(), config.display()),
            format!("mv '{0}' '{0}-moved'", real.display()),
            format!(
                "mv '{0}' '{1}'",
                lead.display(),
                config.join("stolen").display()
            ),
            format!("ln '{}' '{}'", lead.display(), cwd.join("copy").display()),
            format!(
                "ln -s '{}' '{}' && cat '{}'",
                lead.display(),
                cwd.join("sym").display(),
                cwd.join("sym").display()
            ),
            format!("cp '{}' '{}'", lead.display(), cwd.join("copy2").display()),
            // The history file is replaced by a link, or removed.
            format!(
                "rm '{0}/history.jsonl' && ln -s '{1}' '{0}/history.jsonl'",
                config.display(),
                cwd.display()
            ),
            format!("mv '{0}/history.jsonl' '{0}/h2'", config.display()),
        ] {
            let out = run(&rules, &cwd, &script);
            assert!(!out.status.success(), "{script}: {out:?}");
        }
        assert!(lead.exists() && config.exists() && !cwd.join("copy").exists());
        assert!(!cwd.join("copy2").exists() && config.join("history.jsonl").is_file());
        // The worker's own file may be copied out.
        let out = run(
            &rules,
            &cwd,
            &format!("cp '{}' '{}'", mine.display(), cwd.join("mine").display()),
        );
        assert!(out.status.success(), "{out:?}");
    }

    /// Seatbelt cannot tell a directory made by `mkdir` from one moved in by `rename`, so
    /// nothing may be created at `projects` or directly under it, however it is made. Each
    /// attempt prepares a tree outside, with a link inside it, and moves it in.
    #[test]
    fn a_prepared_tree_cannot_be_moved_into_the_claude_folder_under_any_name() {
        let s = setup();
        let base = base_of(&s);
        let config = std::fs::canonicalize(claude_folder(&base)).unwrap();
        let rules = claude_worker(&s, &config, Some("own"));
        let cwd = checkout(&s);
        let out_dir = cwd.join("out");
        let evil = out_dir.join("evil");
        std::fs::create_dir_all(&evil).unwrap();
        let projects = config.join("projects");
        let folder = project_of(&s, &config);
        let future = projects.join("-future-worktree");
        let prep = |name: &str, link: &str| {
            format!(
                "mkdir -p '{o}/{name}' && ln -s '{e}' '{o}/{name}/{link}'",
                o = out_dir.display(),
                e = evil.display()
            )
        };
        let tries = [
            // Under a future project name: a plain link, a directory, a prepared tree.
            format!("ln -s '{}' '{}'", evil.display(), future.display()),
            format!("mkdir '{}'", future.display()),
            format!(
                "{} && mv '{}/p1' '{}'",
                prep("p1", "memory"),
                out_dir.display(),
                future.display()
            ),
            // Over the prepared folder, which is not empty, and under it.
            format!(
                "{} && mv '{}/p2' '{}/-more'",
                prep("p2", "memory"),
                out_dir.display(),
                folder.display()
            ),
            // From inside the allowance: the worker's own dir renamed to another name.
            format!(
                "mkdir -p '{f}/own2' && ln -s '{e}' '{f}/own2/memory' && mv '{f}/own2' '{p}/-Users-x-e'",
                f = folder.display(),
                e = evil.display(),
                p = projects.display()
            ),
            format!(
                "mkdir -p '{f}/own/memory2' && mv '{f}/own' '{f}/-elsewhere'",
                f = folder.display()
            ),
            // The prepared folder itself and `projects` moved aside, or replaced.
            format!("mv '{}' '{}-x'", folder.display(), folder.display()),
            format!("mv '{}' '{}-x'", projects.display(), projects.display()),
            format!("rmdir '{}'", folder.display()),
        ];
        for script in tries {
            let out = run(&rules, &cwd, &script);
            assert!(!out.status.success(), "{script}: {out:?}");
        }
        assert!(std::fs::symlink_metadata(&future).is_err());
        assert!(!folder.join("-more").exists() && !projects.join("-Users-x-e").exists());
        // A write by anyone else under a would-be new folder, or the prepared one, stays unread.
        std::fs::write(folder.join("lead-next.jsonl"), "later").unwrap();
        let out = run(
            &rules,
            &cwd,
            &format!("cat '{}/lead-next.jsonl'", folder.display()),
        );
        assert!(!out.status.success() && out.stdout.is_empty());
        // The worker's own session still works.
        let out = run(
            &rules,
            &cwd,
            &format!(
                "echo ok > '{f}/own.jsonl' && mkdir -p '{f}/own/memory' && cat '{f}/own.jsonl'",
                f = folder.display()
            ),
        );
        assert!(out.status.success(), "{out:?}");
    }

    /// `projects` absent at launch is made by the core, so a worker has no gap to move a
    /// prepared tree into.
    #[test]
    fn a_missing_projects_folder_is_made_by_the_core_and_cannot_be_replaced() {
        let s = setup();
        let base = base_of(&s);
        let fresh = base.join("fresh2");
        std::fs::create_dir_all(&fresh).unwrap();
        let fresh = std::fs::canonicalize(&fresh).unwrap();
        assert!(!fresh.join("projects").exists());
        let rules = claude_worker(&s, &fresh, Some("own"));
        assert!(fresh.join("projects").is_dir());
        let cwd = checkout(&s);
        let evil = cwd.join("evil");
        std::fs::create_dir_all(&evil).unwrap();
        let name = project_of(&s, &fresh);
        let name = name.file_name().unwrap().to_string_lossy().into_owned();
        for script in [
            format!(
                "mkdir -p '{c}/prep' && ln -s '{e}' '{c}/prep/{name}' && mv '{c}/prep' '{p}'",
                c = cwd.display(),
                e = evil.display(),
                p = fresh.join("projects").display()
            ),
            format!(
                "rmdir '{}/projects/{name}' && ln -s '{}' '{}/projects/{name}'",
                fresh.display(),
                evil.display(),
                fresh.display()
            ),
            format!(
                "ln -s '{}' '{}/projects/-x'",
                evil.display(),
                fresh.display()
            ),
        ] {
            let out = run(&rules, &cwd, &script);
            assert!(!out.status.success(), "{script}: {out:?}");
        }
    }

    /// A worker that is not Claude gets the folder closed whole: no session of its own.
    #[test]
    fn a_worker_of_another_engine_gets_no_claude_session_at_all() {
        let s = setup();
        let base = base_of(&s);
        let config = std::fs::canonicalize(claude_folder(&base)).unwrap();
        let rules = s
            .rules
            .clone()
            .with_closed_claude_folder(&config.join("projects"))
            .unwrap();
        assert_eq!(rules.fresh_session(), None);
        let cwd = std::fs::canonicalize(&s.data)
            .unwrap()
            .join("workspace/repo/p");
        for path in [
            "projects/-work-app/lead.jsonl",
            "projects/-work-app/own.jsonl",
            "history.jsonl",
        ] {
            let out = run(
                &rules,
                &cwd,
                &format!("cat '{}'", config.join(path).display()),
            );
            assert!(!out.status.success() && out.stdout.is_empty(), "{path}");
        }
        let out = run(
            &rules,
            &cwd,
            &format!("mkdir '{}/projects/-x'", config.display()),
        );
        assert!(!out.status.success());
        let out = run(
            &rules,
            &cwd,
            &format!("cat '{}'", config.join(".claude.json").display()),
        );
        assert_eq!(out.stdout, b"login state");
        // A Claude folder that is not there yet is named as given and cannot be made a link.
        let fresh = base.join("fresh3");
        std::fs::create_dir_all(&fresh).unwrap();
        let fresh = std::fs::canonicalize(&fresh).unwrap();
        let rules = s
            .rules
            .clone()
            .with_closed_claude_folder(&fresh.join("projects"))
            .unwrap();
        let out = run(
            &rules,
            &cwd,
            &format!("ln -s '{}' '{}/projects'", cwd.display(), fresh.display()),
        );
        assert!(!out.status.success(), "{out:?}");
    }

    #[test]
    fn the_profile_with_transcripts_still_names_no_path_and_protects_the_folders_above() {
        let s = setup();
        let config = std::fs::canonicalize(claude_folder(&base_of(&s))).unwrap();
        let rules = claude_worker(&s, &config, Some("own"));
        let text = macos::profile(&rules);
        assert!(!text.contains(config.to_str().unwrap()));
        assert!(!text.contains("own\""), "the id travels as a parameter");
        assert!(text.contains("param \"TR\"") && text.contains("param \"TS\""));
        let distinct: std::collections::BTreeSet<_> = rules
            .data
            .ancestors()
            .chain(rules.transcripts.as_ref().unwrap().root.ancestors())
            .collect();
        assert_eq!(macos::above_len(&rules), distinct.len());
    }

    /// The worker's normal work in its folders keeps working: absolute `cd`, and the renames
    /// and removals a tool such as git does with lock files.
    #[test]
    fn work_inside_the_checkout_and_run_folder_still_renames_and_removes() {
        let s = setup();
        let data = std::fs::canonicalize(&s.data).unwrap();
        let checkout = data.join("workspace/repo/p");
        let out = run(
            &s.rules,
            &data,
            &format!(
                "cd '{c}' && mkdir -p .git && touch .git/index.lock && mv .git/index.lock .git/index \\
                 && rm .git/index && cd '{r}' && touch a.lock && mv a.lock b && rm b",
                c = checkout.display(),
                r = data.join("runs/mine").display()
            ),
        );
        assert!(out.status.success(), "{out:?}");
    }
}
