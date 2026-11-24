//! A library workflow run uses the app's fixed POSIX `/bin/sh`, never the person's `$SHELL`
//! (docs/execution-profiles-and-library.md, "Parameter substitution is injection-safe"). This is one test in its
//! own test binary because it changes `SHELL` for the whole process, and nothing else may run beside it.

#![cfg(unix)]

#[test]
fn a_non_posix_shell_cannot_change_the_library_runs_interpreter() {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = std::env::temp_dir().join(format!("orbyn-library-shell-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("used");
    let fake = dir.join("not-a-posix-shell");
    std::fs::write(
        &fake,
        format!("#!/bin/sh\n: > '{}'\necho not-posix\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

    // SAFETY: this test binary has one test, so no other thread reads the environment beside this one.
    unsafe { std::env::set_var("SHELL", &fake) };

    // A login profile may print anything first, so mark the answer and read only between
    // the markers rather than trimming the whole output.
    let command = "printf 'ORBYN-SHELL:%s:END' \"$0\"";
    let (program, args) = orbyn_core::host::terminal::library_shell_command(command);
    assert_eq!(program, std::path::PathBuf::from("/bin/sh"));
    assert_eq!(
        args,
        vec![
            std::ffi::OsString::from("-lc"),
            std::ffi::OsString::from(command)
        ]
    );
    let out = std::process::Command::new(&program)
        .args(&args)
        .output()
        .unwrap();
    let printed = String::from_utf8_lossy(&out.stdout);
    let shell = printed
        .split_once("ORBYN-SHELL:")
        .and_then(|(_, rest)| rest.split_once(":END"))
        .map(|(shell, _)| shell)
        .unwrap_or_else(|| panic!("no marked shell answer in {printed:?}"));
    assert!(matches!(shell, "/bin/sh" | "sh"), "{shell}");

    // SAFETY: as above, the one test removes what it set before it ends.
    unsafe { std::env::remove_var("SHELL") };
    assert!(
        !marker.exists(),
        "the person's non-POSIX shell was never started"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
