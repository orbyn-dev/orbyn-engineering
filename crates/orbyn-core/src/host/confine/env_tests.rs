//! What a confined worker's environment carries: no name may keep the app's data folder and no
//! search folder may offer the `orbyn` command line. The last test starts a real shell, so the
//! claim is about what a worker process can find, not about a list of variables.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

use super::*;

/// The variable the service sets for its own data folder.
const DATA_VAR: &str = "ORBYN_DATA_DIR";
/// A second name whose value is a file in the data folder: the rule is about the value, not the name.
const STORE_VAR: &str = "ORBYN_STORE";
/// A variable a confined worker keeps: nothing about it names the data folder.
const KEPT_NAME: &str = "ORBYN_TEST_KEPT";
/// A name the run itself sets on the command: its value here is the one the child gets.
const RUN_VAR: &str = "ORBYN_TEST_RUN_TMP";
/// A variable that names a folder beside the data folder, not inside it.
const SIBLING_VAR: &str = "ORBYN_TEST_SIBLING";
/// The task id a Codex worker's private home is named after.
const TASK: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// The child with no rules, then the worker's child.
const CONTROL: &str = "control";
const WORKER: &str = "worker";
/// The marker that tells the helper test to do the work.
const HELPER: &str = "ORBYN_TEST_WORKER_ENV_HELPER";
const CLI: &str = "orbyn";

/// A data folder with the store in it, and the rules a worker would get for it.
fn setup(dir: &crate::testing::TempDir) -> (PathBuf, Confinement) {
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("runs/mine")).unwrap();
    std::fs::write(data.join("orbyn.sqlite3"), "store").unwrap();
    let rules = Confinement::new(&data, &data.join("runs/mine"), &[], &[]).unwrap();
    (data, rules)
}

/// A data folder and the real Codex home beside it, for the rules that give a worker its own home.
fn codex_setup(dir: &crate::testing::TempDir) -> (PathBuf, Confinement) {
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let data = base.join("data");
    std::fs::create_dir_all(data.join("runs/mine")).unwrap();
    let real = base.join("real-codex");
    std::fs::create_dir_all(&real).unwrap();
    std::fs::write(
        real.join("auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"sk-fake-api-key"}"#,
    )
    .unwrap();
    let rules = Confinement::new(&data, &data.join("runs/mine"), &[], &[])
        .unwrap()
        .with_codex_worker_home_at(TASK, &real)
        .unwrap();
    (data, rules)
}

/// A folder that offers an `orbyn` program, as an install folder does.
fn cli_folder(dir: &crate::testing::TempDir) -> PathBuf {
    let folder = dir.path().join("install");
    std::fs::create_dir_all(&folder).unwrap();
    // A file is enough: the search finds the name, and the rules never run the program.
    std::fs::write(folder.join(CLI), "").unwrap();
    std::fs::write(folder.join(format!("{CLI}.exe")), "").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = folder.join(CLI);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    folder
}

/// What `command` will hand the child for a name: `None` when it is removed, the value when set.
fn env_value<'a>(command: &'a Command, name: &str) -> Option<Option<&'a OsStr>> {
    command
        .get_envs()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}

fn path_entries(value: &OsStr) -> Vec<PathBuf> {
    std::env::split_paths(value).collect()
}

/// On Unix environment names are case-sensitive, so `Path` is a variable of its own: one that names the data
/// folder goes like any other, and is never taken for the search path the rules rewrite.
#[cfg(unix)]
#[test]
fn a_differently_cased_path_is_a_variable_of_its_own_and_goes() {
    let dir = crate::testing::TempDir::new();
    let (data, rules) = setup(&dir);
    let mut command = Command::new("engine");
    command.env("Path", &data).env("PATH", "/usr/bin");
    rules.scrub_env(&mut command);
    assert!(
        command
            .get_envs()
            .any(|(name, value)| name == "Path" && value.is_none()),
        "`Path` names the data folder and is not the search path"
    );
    assert!(
        command
            .get_envs()
            .any(|(name, value)| name == "PATH" && value.is_some()),
        "the search path itself is rewritten, not removed"
    );
}

#[test]
fn a_search_folder_inside_the_data_folder_or_offering_the_cli_goes_and_the_rest_stays() {
    let dir = crate::testing::TempDir::new();
    let (data, rules) = setup(&dir);
    let cli = cli_folder(&dir);
    let keeper = dir.path().join("keeper");
    std::fs::create_dir_all(&keeper).unwrap();
    let inside = data.join("bin");
    std::fs::create_dir_all(&inside).unwrap();
    let path = std::env::join_paths([&cli, &inside, &keeper]).unwrap();

    let mut command = Command::new("engine");
    command.env("PATH", &path);
    rules.scrub_env(&mut command);
    let kept = env_value(&command, "PATH")
        .expect("PATH is decided, never dropped here")
        .expect("PATH is set");
    let entries = path_entries(kept);
    assert_eq!(entries, [keeper], "{entries:?}");
}

#[test]
fn every_name_that_keeps_the_data_folder_goes_and_the_others_stay() {
    let dir = crate::testing::TempDir::new();
    let (data, rules) = setup(&dir);
    let mut command = Command::new("engine");
    command
        .env(DATA_VAR, &data)
        .env(STORE_VAR, data.join("orbyn.sqlite3"))
        .env(KEPT_NAME, "kept")
        .env("HOME", "/home/person");
    rules.scrub_env(&mut command);
    for name in [DATA_VAR, STORE_VAR] {
        assert_eq!(env_value(&command, name), Some(None), "{name} stays");
    }
    for name in [KEPT_NAME, "HOME"] {
        assert!(
            !matches!(env_value(&command, name), Some(None)),
            "{name} goes"
        );
    }
}

/// The Codex home the rules assign a worker is inside the data folder, so the value that carries
/// it must survive the scrub that removes every other route to that folder. The exemption is the
/// value the rules assigned, not the name: a `CODEX_HOME` naming anything else under the data
/// folder is a route to the store like any other.
#[test]
fn a_codex_worker_keeps_the_home_the_rules_assigned_it() {
    let dir = crate::testing::TempDir::new();
    let (data, rules) = codex_setup(&dir);
    let home = rules.codex_home().unwrap().to_path_buf();

    let mut command = Command::new("engine");
    command.env("CODEX_HOME", &home);
    rules.scrub_env(&mut command);
    assert_eq!(
        env_value(&command, "CODEX_HOME"),
        Some(Some(home.as_os_str())),
        "the worker lost the home it was assigned"
    );

    let mut other = Command::new("engine");
    other.env("CODEX_HOME", data.join("elsewhere"));
    rules.scrub_env(&mut other);
    assert_eq!(
        env_value(&other, "CODEX_HOME"),
        Some(None),
        "a CODEX_HOME naming anything else under the data folder stays"
    );
}

/// An inherited name is only a route while it is the value the child gets. The run sets its own
/// variables after the inherited ones, so a safe value it sets for that name is the one that
/// counts; an inherited name with no override keeps going.
#[test]
fn a_safe_value_the_run_sets_for_a_name_survives_the_inherited_one() {
    let dir = crate::testing::TempDir::new();
    let (data, rules) = setup(&dir);
    let inherited = vec![
        (OsString::from(RUN_VAR), data.join("tmp").into_os_string()),
        (
            OsString::from(STORE_VAR),
            data.join("orbyn.sqlite3").into_os_string(),
        ),
    ];
    let mut command = Command::new("engine");
    command.env(RUN_VAR, "safe-tmp");
    rules.scrub_env_with(&mut command, inherited.into_iter());
    assert_eq!(
        env_value(&command, RUN_VAR),
        Some(Some(OsStr::new("safe-tmp"))),
        "the run's own safe value went with the inherited name"
    );
    assert_eq!(
        env_value(&command, STORE_VAR),
        Some(None),
        "an inherited name that keeps the store stays"
    );
}

/// A folder that only starts with the same letters as the data folder is its sibling, not inside
/// it: `/…/orbyn-tools` is not under `/…/orbyn`, so neither its search entry nor a name that
/// carries it is a route to the store.
#[test]
fn a_folder_whose_name_only_starts_like_the_data_folder_is_not_inside_it() {
    let dir = crate::testing::TempDir::new();
    let (data, rules) = setup(&dir);
    let owner = data.with_file_name("data-tools");
    std::fs::create_dir_all(owner.join("bin")).unwrap();
    let sibling = std::fs::canonicalize(owner).unwrap();
    let bin = sibling.join("bin");
    let keeper = dir.path().join("keeper");
    std::fs::create_dir_all(&keeper).unwrap();
    let path = std::env::join_paths([&bin, &keeper]).unwrap();

    let mut command = Command::new("engine");
    command.env("PATH", &path).env(SIBLING_VAR, &sibling);
    rules.scrub_env(&mut command);
    let kept = path_entries(
        env_value(&command, "PATH")
            .expect("PATH is decided, never dropped here")
            .expect("PATH is set"),
    );
    assert_eq!(kept, [bin, keeper], "{kept:?}");
    assert_eq!(
        env_value(&command, SIBLING_VAR),
        Some(Some(sibling.as_os_str())),
        "a name carrying the sibling folder stays"
    );
}

/// A real program that prints its environment.
fn env_printer() -> Command {
    if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/D", "/C", "set"]);
        command
    } else {
        Command::new("/usr/bin/env")
    }
}

/// A real program that looks `orbyn` up the way a worker's shell would: success means it was found.
fn lookup_probe() -> Command {
    if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/D", "/C", "where", CLI]);
        command
    } else {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "command -v orbyn"]);
        command
    }
}

fn section(text: &str, kind: &str, what: &str) -> String {
    let begin = text
        .find(&format!("BEGIN {what} {kind}"))
        .unwrap_or_else(|| panic!("the helper printed {what} {kind}: {text}"));
    let end = text
        .find(&format!("END {what} {kind}"))
        .unwrap_or_else(|| panic!("the helper finished {what} {kind}: {text}"));
    text[begin..end].to_owned()
}

fn found(text: &str, kind: &str) -> bool {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("STATUS CLINAME {kind} ")))
        .is_some_and(|answer| answer.trim() == "true")
}

/// Runs only inside the process the next test starts: it starts real children that print their
/// environment and look `orbyn` up, once as they are and once with a worker's rules.
#[test]
fn helper_prints_what_a_real_shell_can_find() {
    if std::env::var_os(HELPER).is_none() {
        return;
    }
    let data = PathBuf::from(std::env::var_os(DATA_VAR).expect("the data folder is given"));
    let rules = Confinement::new(&data, &data.join("runs/mine"), &[], &[]).expect("the rules");
    for (kind, worker) in [(CONTROL, false), (WORKER, true)] {
        for (what, mut command) in [("ENV", env_printer()), ("CLINAME", lookup_probe())] {
            if worker {
                rules.scrub_env(&mut command);
            }
            let output = command.output().expect("the child ran");
            let status = output.status.success();
            println!("BEGIN {what} {kind}");
            println!("{}", String::from_utf8_lossy(&output.stdout));
            println!("{}", String::from_utf8_lossy(&output.stderr));
            println!("END {what} {kind}");
            if what == "CLINAME" {
                println!("STATUS CLINAME {kind} {status}");
            }
        }
    }
}

#[test]
fn a_real_worker_shell_cannot_find_the_cli_or_the_data_folder_and_keeps_the_rest() {
    let dir = crate::testing::TempDir::new();
    let (data, _) = setup(&dir);
    let cli = cli_folder(&dir);
    let keeper = dir.path().join("keeper");
    std::fs::create_dir_all(&keeper).unwrap();
    let mut entries = vec![cli.clone(), data.clone(), keeper.clone()];
    entries.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(&entries).unwrap();
    let kept = format!("kept-{}", crate::util::new_id());

    let mut helper = Command::new(std::env::current_exe().unwrap());
    helper
        .args([
            "--exact",
            "host::confine::env_tests::helper_prints_what_a_real_shell_can_find",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HELPER, "1")
        .env(DATA_VAR, &data)
        .env(STORE_VAR, data.join("orbyn.sqlite3"))
        .env(KEPT_NAME, &kept)
        .env("PATH", &path);
    let output = helper.output().expect("the helper ran");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "{text}");

    // The same shell with no rules finds the CLI by name and is handed the data folder: the
    // attempt is a real one, not one that would fail anyway.
    assert!(
        found(&text, CONTROL),
        "control did not find the CLI: {text}"
    );
    let control = section(&text, CONTROL, "ENV");
    assert!(
        contains(&control, &data.to_string_lossy()),
        "control was not handed the data folder: {control}"
    );

    // With the worker's rules neither is there.
    assert!(
        !found(&text, WORKER),
        "a worker's shell found the CLI: {}",
        section(&text, WORKER, "CLINAME")
    );
    let worker = section(&text, WORKER, "ENV");
    assert!(
        !contains(&worker, &data.to_string_lossy()),
        "a worker's environment kept the data folder: {worker}"
    );
    assert!(
        !contains(&worker, &cli.to_string_lossy()),
        "a worker's environment kept the CLI's folder: {worker}"
    );
    // Everything else is still there.
    for kept_text in [
        format!("{KEPT_NAME}={kept}"),
        keeper.to_string_lossy().into_owned(),
    ] {
        assert!(contains(&worker, &kept_text), "{kept_text} went: {worker}");
    }
}

/// Whether `text` holds `wanted`, without regard to case: Windows spells a path either way.
fn contains(text: &str, wanted: &str) -> bool {
    text.to_lowercase().contains(&wanted.to_lowercase())
}
