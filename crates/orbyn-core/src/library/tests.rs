//! The personal library: the rules of a save, the keychain boundary, injection-safe rendering, the grants a run needs, the masking of a
//! run's output and the reading of committed `.orbyn/` files. A memory keychain stands in for the real one in every test.

use std::collections::BTreeMap;
use std::sync::Arc;

use orbyn_contracts::{
    CopySuggestion, EnvSetDraft, LibraryKind, Param, ParamKind, ProfileGrant, PromptDraft,
    RenderValues, RunWorkflow, UpdateEnvSet, UpdateWorkflow, WorkflowDraft,
};
use serde_json::{Value, json};

use super::*;
use crate::app::CoreConfig;
use crate::credentials::{ENV_DEV_SERVICE, EnvSets, Failure, MemoryBackend};
use crate::library::params::{Shell, quote};
use crate::redaction::LiteralMasker;
use crate::store::{NewProject, NewThread};
use crate::testing::TempDir;

const SECRET: &str = "s3cr3t-Value/with+odd=chars-7731";

struct Fixture {
    dir: TempDir,
    core: Core,
    memory: Arc<MemoryBackend>,
    thread: String,
    project: String,
}

fn fixture() -> Fixture {
    let dir = TempDir::new();
    let mut core = Core::open(CoreConfig::new(
        dir.path().join("data"),
        "http://127.0.0.1:1".into(),
    ))
    .unwrap();
    let memory = Arc::new(MemoryBackend::default());
    core.env_sets = EnvSets::new(memory.clone(), ENV_DEV_SERVICE);
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let project = core
        .store
        .add_project(NewProject {
            path: &work.to_string_lossy(),
            name: "p",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    core.store.trust_project(&project.id, "user").unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id.clone(),
            title: "t".into(),
            engine: "claude".into(),
            model: None,
            cwd: work.to_string_lossy().into_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    Fixture {
        dir,
        core,
        memory,
        thread: thread.id,
        project: project.id,
    }
}

fn code(error: &CoreError) -> (u16, &'static str) {
    (error.status(), error.code())
}

fn param(name: &str, kind: ParamKind) -> Param {
    Param {
        name: name.into(),
        label: name.into(),
        kind,
        default: None,
        required: false,
        choices: None,
    }
}

fn wf(name: &str, command: &str, params: Vec<Param>, sets: Vec<String>) -> WorkflowDraft {
    WorkflowDraft {
        name: name.into(),
        description: String::new(),
        command: command.into(),
        params,
        env_set_ids: sets,
    }
}

fn values(pairs: &[(&str, &str)]) -> RenderValues {
    RenderValues {
        values: pairs
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
    }
}

fn audit(core: &Core, action: &str) -> Vec<orbyn_contracts::AuditEvent> {
    core.store
        .list_audit(None, 500)
        .unwrap()
        .into_iter()
        .filter(|row| row.action == action)
        .collect()
}

fn env_set_with_value(f: &Fixture, name: &str, key: &str) -> EnvSet {
    let set = f
        .core
        .create_env_set(&EnvSetDraft {
            name: name.into(),
            description: String::new(),
            keys: vec![key.into()],
        })
        .unwrap();
    f.core.set_env_key(&set.id, key, SECRET).unwrap();
    set
}

/// The stored presence of each key of a set, in order.
fn stored_flags(f: &Fixture, set_id: &str) -> Vec<Option<bool>> {
    let reader = f.core.store.reader_for_test();
    let mut statement = reader
        .prepare("SELECT is_set FROM library_env_keys WHERE set_id = ?1 ORDER BY rowid")
        .unwrap();
    statement
        .query_map([set_id], |row| row.get::<_, Option<bool>>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Leaves the keys as migration 72 found them: present unknown.
fn forget_flags(f: &Fixture, set_id: &str) {
    f.core
        .store
        .writer_for_test()
        .execute(
            "UPDATE library_env_keys SET is_set = NULL WHERE set_id = ?1",
            [set_id],
        )
        .unwrap();
}

fn run_body(thread: &str, ack: Vec<ProfileGrant>) -> RunWorkflow {
    RunWorkflow {
        values: BTreeMap::new(),
        thread_id: Some(thread.into()),
        project_id: None,
        ack_grants: ack,
    }
}

// ---- the mask ----

fn mask_all(values: &[(&str, &str)], chunks: &[&[u8]]) -> Vec<u8> {
    let mut masker = LiteralMasker::new(values).unwrap();
    let mut out = Vec::new();
    for chunk in chunks {
        out.extend(masker.push(chunk));
    }
    out.extend(masker.finish());
    out
}

#[test]
fn a_value_split_at_every_point_never_shows_and_is_hidden_by_key() {
    let text = format!("before {SECRET} after\n");
    let bytes = text.as_bytes();
    for at in 0..=bytes.len() {
        let out = mask_all(&[("API_TOKEN", SECRET)], &[&bytes[..at], &bytes[at..]]);
        let shown = String::from_utf8(out).unwrap();
        assert!(!shown.contains(SECRET), "split at {at}: {shown}");
        assert_eq!(
            shown, "before [hidden: env API_TOKEN] after\n",
            "split at {at}"
        );
    }
}

#[test]
fn a_value_split_across_three_chunks_and_byte_by_byte_is_masked() {
    let text = format!("x{SECRET}y");
    let one_by_one: Vec<&[u8]> = text.as_bytes().chunks(1).collect();
    let shown = String::from_utf8(mask_all(&[("K", SECRET)], &one_by_one)).unwrap();
    assert_eq!(shown, "x[hidden: env K]y");
    let third = text.len() / 3;
    let b = text.as_bytes();
    let shown = String::from_utf8(mask_all(
        &[("K", SECRET)],
        &[&b[..third], &b[third..2 * third], &b[2 * third..]],
    ))
    .unwrap();
    assert_eq!(shown, "x[hidden: env K]y");
}

#[test]
fn the_mask_holds_back_only_what_could_still_become_a_value() {
    let mut masker = LiteralMasker::new(&[("K", "hunter2")]).unwrap();
    assert_eq!(masker.push(b"plain output\n"), b"plain output\n");
    assert_eq!(masker.push(b"say hun"), b"say ");
    assert_eq!(masker.push(b"ter2 now"), b"[hidden: env K] now");
    assert_eq!(masker.push(b"hun"), b"");
    assert_eq!(masker.finish(), b"hun");
}

#[test]
fn two_values_overlapping_and_a_multibyte_split_are_masked() {
    let shown = String::from_utf8(mask_all(
        &[("A", "abc"), ("B", "abcdef")],
        &[b"x abcd", b"ef y abc"],
    ))
    .unwrap();
    assert_eq!(shown, "x [hidden: env B] y [hidden: env A]");
    let value = "pässwörd-é";
    let bytes = format!("<{value}>").into_bytes();
    for at in 0..=bytes.len() {
        let shown =
            String::from_utf8(mask_all(&[("K", value)], &[&bytes[..at], &bytes[at..]])).unwrap();
        assert_eq!(shown, "<[hidden: env K]>", "split at {at}");
    }
}

#[test]
fn a_multi_line_value_is_masked_as_a_terminal_writes_it() {
    let shown =
        String::from_utf8(mask_all(&[("K", "line1\nline2")], &[b"a line1\r\nline2 b"])).unwrap();
    assert_eq!(shown, "a [hidden: env K] b");
}

// ---- the keychain boundary ----

#[test]
fn a_value_goes_to_the_keychain_and_nowhere_else() {
    let f = fixture();
    let set = env_set_with_value(&f, "Staging", "API_TOKEN");
    // The keychain holds it, in the env-sets namespace, under `<set id>/<KEY>`.
    assert_eq!(
        f.memory
            .bytes(ENV_DEV_SERVICE, &format!("{}/API_TOKEN", set.id))
            .as_deref(),
        Some(SECRET.as_bytes())
    );
    assert!(
        f.memory
            .accounts()
            .iter()
            .all(|(service, _)| service == ENV_DEV_SERVICE)
    );
    // Nothing in the data folder (SQLite, its journal, any file) holds the bytes.
    let mut files = vec![f.dir.path().join("data")];
    while let Some(path) = files.pop() {
        if path.is_dir() {
            files.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
        } else {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()),
                "the value is in {}",
                path.display()
            );
        }
    }
    // No answer, audit row or error holds it.
    let everything = json!({
        "summary": f.core.library_summary().unwrap(),
        "list": f.core.list_env_sets().unwrap(),
        "one": f.core.get_env_set(&set.id).unwrap(),
        "audit": f.core.store.list_audit(None, 500).unwrap(),
        "export": f.core.export_library_sync_for_test(),
    })
    .to_string();
    assert!(!everything.contains(SECRET));
    let set = f.core.get_env_set(&set.id).unwrap();
    assert!(set.keys[0].present);
}

impl Core {
    fn export_library_sync_for_test(&self) -> Value {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        serde_json::to_value(runtime.block_on(self.export_library(None)).unwrap()).unwrap()
    }
}

#[test]
fn the_schema_has_no_column_that_could_hold_a_value() {
    let f = fixture();
    let reader = f.core.store.reader_for_test();
    for table in [
        "library_workflows",
        "library_prompts",
        "library_env_sets",
        "library_env_keys",
    ] {
        let mut statement = reader
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .unwrap();
        let columns: Vec<String> = statement
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        for column in &columns {
            assert!(
                ![
                    "value", "secret", "token", "data", "bytes", "blob", "password"
                ]
                .iter()
                .any(|bad| column.contains(bad)),
                "{table}.{column} could hold a value"
            );
        }
    }
    let mut statement = reader
        .prepare("SELECT name FROM pragma_table_info('library_env_keys')")
        .unwrap();
    let columns: Vec<String> = statement
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        columns,
        ["set_id", "key", "created_at", "updated_at", "is_set"]
    );
    let mut statement = reader
        .prepare("SELECT name FROM pragma_table_info('library_env_sets')")
        .unwrap();
    let columns: Vec<String> = statement
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        columns,
        [
            "id",
            "name",
            "name_key",
            "description",
            "revision",
            "created_at",
            "updated_at"
        ]
    );
}

#[test]
fn a_secret_prints_nothing_in_debug() {
    let f = fixture();
    let set = env_set_with_value(&f, "S", "K");
    let got = f
        .core
        .env_sets
        .spawn_values(&set.id, &["K".to_owned()])
        .unwrap();
    assert!(!format!("{got:?}").contains(SECRET));
    let run = crate::host::terminal::RunSecrets {
        mask: LiteralMasker::new(&[("K", SECRET)]).unwrap(),
        env: got,
    };
    assert!(!format!("{run:?}").contains(SECRET));
    assert!(
        !format!(
            "{:?}",
            orbyn_contracts::SetEnvValue {
                value: SECRET.into()
            }
        )
        .contains(SECRET)
    );
}

#[test]
fn key_names_follow_the_rules() {
    let f = fixture();
    let make = |keys: Vec<&str>| {
        f.core.create_env_set(&EnvSetDraft {
            name: format!("set {}", keys.len()),
            description: String::new(),
            keys: keys.into_iter().map(String::from).collect(),
        })
    };
    for bad in [
        "PATH",
        "HOME",
        "LD_PRELOAD",
        "DYLD_X",
        "NODE_OPTIONS",
        "ORBYN_TOKEN",
        "CLAUDE_CODE_X",
        "CODEXHOME",
        "CURSOR_API_KEY",
        "BASH_ENV",
    ] {
        assert_eq!(
            code(&make(vec![bad]).unwrap_err()),
            (409, "key_refused"),
            "{bad}"
        );
    }
    assert_eq!(
        code(&make(vec!["lower"]).unwrap_err()),
        (400, "invalid_input")
    );
    assert_eq!(
        code(&make(vec!["A", "A"]).unwrap_err()),
        (409, "key_refused")
    );
    let many: Vec<String> = (0..65).map(|n| format!("K{n}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    assert_eq!(code(&make(many).unwrap_err()), (409, "library_limit"));
    assert!(make(vec!["API_BASE", "_X1"]).is_ok());
}

#[test]
fn no_keychain_means_planned_and_503_and_a_blocked_one_is_retryable() {
    let f = fixture();
    let set = f
        .core
        .create_env_set(&EnvSetDraft {
            name: "S".into(),
            description: String::new(),
            keys: vec!["K".into()],
        })
        .unwrap();
    assert_eq!(
        (set.keychain, set.status),
        (EnvKeychain::Ok, EnvSetStatus::Stored)
    );
    f.memory.fail_with(Some(Failure::Unusable));
    let read = f.core.get_env_set(&set.id).unwrap();
    assert_eq!(
        (read.keychain, read.status, read.keys[0].present),
        (EnvKeychain::Unavailable, EnvSetStatus::Planned, false)
    );
    assert_eq!(
        f.core.library_summary().unwrap().keychain,
        EnvKeychain::Unavailable
    );
    let error = f.core.set_env_key(&set.id, "K", SECRET).unwrap_err();
    assert_eq!(code(&error), (503, "keychain_unavailable"));
    assert!(!error.to_string().contains(SECRET));
    f.memory.fail_with(Some(Failure::Blocked));
    assert_eq!(
        code(&f.core.set_env_key(&set.id, "K", SECRET).unwrap_err()),
        (503, "keychain_blocked")
    );
    assert_eq!(
        f.core.get_env_set(&set.id).unwrap().keychain,
        EnvKeychain::Blocked
    );
    f.memory.fail_with(Some(Failure::Other));
    assert_eq!(
        code(&f.core.set_env_key(&set.id, "K", SECRET).unwrap_err()),
        (502, "keychain_failed")
    );
    f.memory.fail_with(None);
    assert!(f.core.set_env_key(&set.id, "K", SECRET).is_ok());
}

#[test]
fn a_value_of_the_wrong_size_or_for_an_unknown_key_is_refused_without_echo() {
    let f = fixture();
    let set = f
        .core
        .create_env_set(&EnvSetDraft {
            name: "S".into(),
            description: String::new(),
            keys: vec!["K".into()],
        })
        .unwrap();
    assert_eq!(
        code(&f.core.set_env_key(&set.id, "K", "").unwrap_err()),
        (400, "invalid_input")
    );
    let long = "x".repeat(8193);
    let error = f.core.set_env_key(&set.id, "K", &long).unwrap_err();
    assert_eq!(code(&error), (400, "invalid_input"));
    assert!(!error.to_string().contains(&long));
    assert!(f.core.set_env_key(&set.id, "K", &"x".repeat(8192)).is_ok());
    assert_eq!(
        code(&f.core.set_env_key(&set.id, "K", "a\0b").unwrap_err()),
        (400, "invalid_input")
    );
    assert_eq!(
        code(&f.core.set_env_key(&set.id, "OTHER", SECRET).unwrap_err()),
        (404, "not_found")
    );
    assert_eq!(
        f.core.remove_env_key(&set.id, "K").unwrap(),
        EnvKey {
            name: "K".into(),
            present: false
        }
    );
    assert!(!f.core.get_env_set(&set.id).unwrap().keys[0].present);
}

#[test]
fn deleting_and_removing_names_removes_items_first_and_a_failure_leaves_the_set() {
    let f = fixture();
    let set = env_set_with_value(&f, "S", "K");
    f.memory.fail_with(Some(Failure::Other));
    assert_eq!(
        code(&f.core.delete_env_set(&set.id).unwrap_err()),
        (502, "keychain_failed")
    );
    f.memory.fail_with(None);
    assert_eq!(f.core.get_env_set(&set.id).unwrap().keys.len(), 1);
    assert_eq!(f.memory.accounts().len(), 1);
    let renamed = f
        .core
        .update_env_set(
            &set.id,
            &UpdateEnvSet {
                seen_revision: 1,
                name: None,
                description: None,
                keys: Some(vec!["B".into()]),
            },
        )
        .unwrap();
    assert_eq!(
        renamed
            .keys
            .iter()
            .map(|k| k.name.as_str())
            .collect::<Vec<_>>(),
        ["B"]
    );
    assert!(
        f.memory.accounts().is_empty(),
        "a removed name loses its keychain item"
    );
    let set = env_set_with_value(&f, "T", "K");
    f.core.delete_env_set(&set.id).unwrap();
    assert!(f.memory.accounts().is_empty());
    assert_eq!(
        code(&f.core.get_env_set(&set.id).unwrap_err()),
        (404, "not_found")
    );
}

#[test]
fn listing_three_stored_keys_reads_no_key_from_the_keychain() {
    let f = fixture();
    let names = ["API_BASE", "API_TOKEN", "STAGING_URL"];
    let set = f
        .core
        .create_env_set(&EnvSetDraft {
            name: "Staging".into(),
            description: String::new(),
            keys: names.iter().map(|name| (*name).to_owned()).collect(),
        })
        .unwrap();
    for name in &names {
        f.core.set_env_key(&set.id, name, SECRET).unwrap();
    }
    let reads = f.memory.get_calls();
    let listed = f.core.list_env_sets().unwrap();
    // The one `get` is the keychain state every read asks for; no key was read.
    assert_eq!(
        f.memory.get_calls() - reads,
        1,
        "the keychain state only, zero key reads"
    );
    assert_eq!(
        listed[0].keys,
        names
            .iter()
            .map(|name| EnvKey {
                name: (*name).into(),
                present: true,
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        stored_flags(&f, &set.id),
        [Some(true), Some(true), Some(true)]
    );
}

#[test]
fn a_removed_key_reads_false_from_the_record_without_a_keychain_read() {
    let f = fixture();
    let set = f
        .core
        .create_env_set(&EnvSetDraft {
            name: "Staging".into(),
            description: String::new(),
            keys: vec!["A".into(), "B".into()],
        })
        .unwrap();
    f.core.set_env_key(&set.id, "A", SECRET).unwrap();
    f.core.set_env_key(&set.id, "B", SECRET).unwrap();
    f.core.remove_env_key(&set.id, "A").unwrap();
    let reads = f.memory.get_calls();
    let listed = f.core.list_env_sets().unwrap();
    // The one `get` is the keychain state every read asks for; no key was read.
    assert_eq!(
        f.memory.get_calls() - reads,
        1,
        "the keychain state only, zero key reads"
    );
    assert_eq!(
        listed[0].keys,
        [
            EnvKey {
                name: "A".into(),
                present: false,
            },
            EnvKey {
                name: "B".into(),
                present: true,
            },
        ]
    );
    assert_eq!(stored_flags(&f, &set.id), [Some(false), Some(true)]);
}

#[test]
fn an_unknown_presence_is_probed_once_then_recorded() {
    let f = fixture();
    let set = env_set_with_value(&f, "S", "K");
    forget_flags(&f, &set.id);
    assert_eq!(stored_flags(&f, &set.id), [None]);

    let reads = f.memory.get_calls();
    let first = f.core.list_env_sets().unwrap();
    assert!(first[0].keys[0].present);
    // The keychain state and the one unknown key; its answer is now recorded.
    assert_eq!(f.memory.get_calls() - reads, 2);
    assert_eq!(stored_flags(&f, &set.id), [Some(true)]);

    let reads = f.memory.get_calls();
    let second = f.core.list_env_sets().unwrap();
    assert!(second[0].keys[0].present);
    assert_eq!(
        f.memory.get_calls() - reads,
        1,
        "the keychain state only, no key read"
    );
}

#[test]
fn an_unusable_keychain_reads_false_and_keeps_the_record() {
    let f = fixture();
    let set = env_set_with_value(&f, "S", "K");
    assert_eq!(stored_flags(&f, &set.id), [Some(true)]);
    f.memory.fail_with(Some(Failure::Unusable));
    let reads = f.memory.get_calls();
    let listed = f.core.list_env_sets().unwrap();
    // The state check fails straight away; no key is read and the record is left as it was.
    assert_eq!(f.memory.get_calls() - reads, 1);
    assert_eq!(
        (listed[0].keychain, listed[0].keys[0].present),
        (EnvKeychain::Unavailable, false)
    );
    assert_eq!(stored_flags(&f, &set.id), [Some(true)]);
    f.memory.fail_with(None);
    assert!(f.core.list_env_sets().unwrap()[0].keys[0].present);
}

#[test]
fn the_single_read_rechecks_the_keychain_and_records_it() {
    let f = fixture();
    let set = env_set_with_value(&f, "S", "K");
    assert!(f.core.list_env_sets().unwrap()[0].keys[0].present);
    // Removed outside the app: the store still says a value was saved.
    f.core.env_sets.remove(&set.id, "K").unwrap();
    assert!(
        f.core.list_env_sets().unwrap()[0].keys[0].present,
        "the list reads the app's own record"
    );
    let one = f.core.get_env_set(&set.id).unwrap();
    assert!(
        !one.keys[0].present,
        "the single read re-checks the keychain"
    );
    assert_eq!(stored_flags(&f, &set.id), [Some(false)]);
    assert!(!f.core.list_env_sets().unwrap()[0].keys[0].present);
}

#[test]
fn an_invalid_value_changes_no_presence_flag() {
    let f = fixture();
    let set = f
        .core
        .create_env_set(&EnvSetDraft {
            name: "S".into(),
            description: String::new(),
            keys: vec!["K".into()],
        })
        .unwrap();
    assert_eq!(stored_flags(&f, &set.id), [Some(false)]);
    assert_eq!(
        code(&f.core.set_env_key(&set.id, "K", "").unwrap_err()),
        (400, "invalid_input")
    );
    assert_eq!(stored_flags(&f, &set.id), [Some(false)]);
    f.core.set_env_key(&set.id, "K", SECRET).unwrap();
    assert_eq!(stored_flags(&f, &set.id), [Some(true)]);
    assert_eq!(
        code(&f.core.set_env_key(&set.id, "K", "").unwrap_err()),
        (400, "invalid_input")
    );
    assert_eq!(stored_flags(&f, &set.id), [Some(true)]);
}

// ---- parameters and rendering ----

#[cfg(unix)]
fn shell_prints(command: &str) -> String {
    let out = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[cfg(unix)]
#[test]
fn a_value_with_shell_metacharacters_is_one_word_and_adds_no_command() {
    let f = fixture();
    let marker = f.dir.path().join("pwned");
    let hostile = [
        format!("x'; touch {} #", marker.display()),
        format!("$(touch {})", marker.display()),
        format!("`touch {}`", marker.display()),
        format!("a & touch {}", marker.display()),
        format!("a | touch {}", marker.display()),
        format!("a\ntouch {}", marker.display()),
        "it's \"quoted\" \\ $HOME ${HOME} * ? ~ ; > < ( )".to_owned(),
        "'".to_owned(),
        "''".to_owned(),
    ];
    let workflow = f
        .core
        .create_workflow(&wf(
            "Echo",
            "printf '%s' {{v}}",
            vec![param("v", ParamKind::Text)],
            vec![],
        ))
        .unwrap();
    for value in hostile {
        let rendered = f
            .core
            .render_workflow(&workflow.id, &values(&[("v", &value)]))
            .unwrap()
            .command;
        assert_eq!(shell_prints(&rendered), value, "{rendered}");
        assert!(!marker.exists());
    }
}

#[test]
fn a_placeholder_inside_the_commands_own_quotes_or_a_heredoc_is_refused_at_save() {
    let f = fixture();
    let p = || vec![param("v", ParamKind::Text)];
    for bad in [
        "echo \"{{v}}\"",
        "echo '{{v}}'",
        "echo $'{{v}}'",
        "echo \"a \\\" {{v}}\"",
        "cat <<EOF\n{{v}}\nEOF",
    ] {
        let error = f
            .core
            .create_workflow(&wf("W", bad, p(), vec![]))
            .unwrap_err();
        assert_eq!(code(&error), (400, "invalid_input"), "{bad}");
    }
    assert!(
        f.core
            .create_workflow(&wf("A", "echo 'it''s' {{v}} \"ok\"", p(), vec![]))
            .is_ok()
    );
    assert!(
        f.core
            .create_workflow(&wf("B", "docker ps --format '{{.ID}}' {{v}}", p(), vec![]))
            .is_ok()
    );
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("C", "echo {{missing}}", p(), vec![]))
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
}

#[test]
fn rendering_checks_required_choices_paths_and_unknown_values() {
    let f = fixture();
    let mut choice = param("mode", ParamKind::Choice);
    choice.choices = Some(vec!["fast".into(), "slow".into()]);
    choice.required = true;
    let mut path = param("dir", ParamKind::Path);
    path.default = Some("src".into());
    let optional = param("opt", ParamKind::Text);
    let workflow = f
        .core
        .create_workflow(&wf(
            "W",
            "run {{mode}} {{dir}} {{opt}} --",
            vec![choice, path, optional],
            vec![],
        ))
        .unwrap();
    let render = |pairs: &[(&str, &str)]| f.core.render_workflow(&workflow.id, &values(pairs));
    assert_eq!(code(&render(&[]).unwrap_err()), (400, "invalid_input"));
    assert_eq!(
        code(&render(&[("mode", "medium")]).unwrap_err()),
        (400, "invalid_input")
    );
    // A value is quoted for the shell this platform runs (params.rs): `'fast'` under a POSIX
    // shell, `"fast"` under `cmd.exe`. The expectation comes from that one rule, so it cannot
    // drift from the product.
    let quoted = |value: &str| quote(value, Shell::native()).expect("the test values are quotable");
    assert_eq!(
        render(&[("mode", "fast")]).unwrap().command,
        format!("run {} {}  --", quoted("fast"), quoted("src"))
    );
    for bad in ["../x", "a/../b", "/etc/passwd", "-rf", "C:\\x", "~/x"] {
        assert_eq!(
            code(&render(&[("mode", "fast"), ("dir", bad)]).unwrap_err()),
            (400, "invalid_input"),
            "{bad}"
        );
    }
    assert_eq!(
        code(&render(&[("mode", "fast"), ("nope", "1")]).unwrap_err()),
        (400, "invalid_input")
    );
    assert_eq!(
        code(&render(&[("mode", "fast"), ("opt", &"x".repeat(4001))]).unwrap_err()),
        (400, "invalid_input")
    );
}

#[test]
fn a_prompt_fills_its_values_as_typed_in_one_pass() {
    let f = fixture();
    let prompt = f
        .core
        .create_prompt(&PromptDraft {
            name: "P".into(),
            description: String::new(),
            body: "Explain {{error}} and {{other}}.".into(),
            params: vec![
                param("error", ParamKind::Text),
                param("other", ParamKind::Text),
            ],
        })
        .unwrap();
    let text = f
        .core
        .render_prompt(
            &prompt.id,
            &values(&[("error", "{{other}} $(x)"), ("other", "B")]),
        )
        .unwrap()
        .text;
    assert_eq!(text, "Explain {{other}} $(x) and B.");
}

#[test]
fn parameters_are_limited_and_shaped() {
    let f = fixture();
    let many: Vec<Param> = (0..17)
        .map(|n| param(&format!("p{n}"), ParamKind::Text))
        .collect();
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("A", "x", many, vec![]))
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
    for name in ["Bad", "1a", "a-b", &"a".repeat(33)] {
        let error = f
            .core
            .create_workflow(&wf("B", "x", vec![param(name, ParamKind::Text)], vec![]))
            .unwrap_err();
        assert_eq!(code(&error), (400, "invalid_input"), "{name}");
    }
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf(
                    "C",
                    "x",
                    vec![param("a", ParamKind::Text), param("a", ParamKind::Text)],
                    vec![]
                ))
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
    let mut one_choice = param("c", ParamKind::Choice);
    one_choice.choices = Some(vec!["only".into()]);
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("D", "x", vec![one_choice], vec![]))
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
    let mut long_default = param("d", ParamKind::Text);
    long_default.default = Some("x".repeat(501));
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("E", "x", vec![long_default], vec![]))
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
}

// ---- limits, names and revisions ----

#[test]
fn counts_sizes_names_and_revisions_are_enforced() {
    let f = fixture();
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("Big", &"x".repeat(4001), vec![], vec![]))
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
    assert!(
        f.core
            .create_workflow(&wf("Max", &"x".repeat(4000), vec![], vec![]))
            .is_ok()
    );
    assert_eq!(
        code(
            &f.core
                .create_prompt(&PromptDraft {
                    name: "P".into(),
                    description: String::new(),
                    body: "x".repeat(32 * 1024 + 1),
                    params: vec![]
                })
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("MAX", "y", vec![], vec![]))
                .unwrap_err()
        ),
        (409, "name_taken")
    );
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("", "y", vec![], vec![]))
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("X", "y", vec![], vec!["nope".into()]))
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
    let one = f
        .core
        .create_workflow(&wf("One", "y", vec![], vec![]))
        .unwrap();
    let patch = |seen, command: &str| UpdateWorkflow {
        seen_revision: seen,
        name: None,
        description: None,
        command: Some(command.into()),
        params: None,
        env_set_ids: None,
    };
    assert_eq!(
        f.core
            .update_workflow(&one.id, &patch(1, "z"))
            .unwrap()
            .revision,
        2
    );
    assert_eq!(
        code(&f.core.update_workflow(&one.id, &patch(1, "w")).unwrap_err()),
        (409, "revision_conflict")
    );
    for n in 0..198 {
        f.core
            .create_workflow(&wf(&format!("w{n}"), "y", vec![], vec![]))
            .unwrap();
    }
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("over", "y", vec![], vec![]))
                .unwrap_err()
        ),
        (409, "library_limit")
    );
    assert_eq!(f.core.library_summary().unwrap().workflows, 200);
    for n in 0..50 {
        f.core
            .create_env_set(&EnvSetDraft {
                name: format!("s{n}"),
                description: String::new(),
                keys: vec![],
            })
            .unwrap();
    }
    assert_eq!(
        code(
            &f.core
                .create_env_set(&EnvSetDraft {
                    name: "over".into(),
                    description: String::new(),
                    keys: vec![]
                })
                .unwrap_err()
        ),
        (409, "library_limit")
    );
    let sets = f.core.list_env_sets().unwrap();
    let ids: Vec<String> = sets.iter().take(5).map(|s| s.id.clone()).collect();
    assert_eq!(
        code(
            &f.core
                .create_workflow(&wf("sets", "y", vec![], ids))
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
    let limits = serde_json::to_value(f.core.library_summary().unwrap().limits).unwrap();
    assert_eq!(limits["import_bytes"], 2_097_152);
    assert_eq!(limits["env_value_bytes"], 8192);
}

#[test]
fn deleting_a_set_removes_it_from_the_workflows_that_listed_it() {
    let f = fixture();
    let set = f
        .core
        .create_env_set(&EnvSetDraft {
            name: "S".into(),
            description: String::new(),
            keys: vec![],
        })
        .unwrap();
    let w = f
        .core
        .create_workflow(&wf("W", "y", vec![], vec![set.id.clone()]))
        .unwrap();
    f.core.delete_env_set(&set.id).unwrap();
    assert!(f.core.get_workflow(&w.id).unwrap().env_set_ids.is_empty());
}

// ---- grants: env values and untrusted sources ----

#[cfg(unix)]
#[tokio::test]
async fn a_run_with_sets_needs_the_env_values_ack_and_opens_no_terminal_without_it() {
    let f = fixture();
    let set = env_set_with_value(&f, "Staging", "API_TOKEN");
    let w = f
        .core
        .create_workflow(&wf(
            "Show",
            "printenv API_TOKEN",
            vec![],
            vec![set.id.clone()],
        ))
        .unwrap();
    let error = f
        .core
        .run_workflow(&w.id, &run_body(&f.thread, vec![]))
        .await
        .unwrap_err();
    assert_eq!(code(&error), (409, "confirm_required"));
    let CoreError::Refused { detail, .. } = &error else {
        panic!()
    };
    assert_eq!(detail["grants"], json!(["env_values"]));
    assert_eq!(
        detail["env_sets"],
        json!([{ "name": "Staging", "keys": ["API_TOKEN"] }])
    );
    assert!(!detail.to_string().contains(SECRET) && !error.to_string().contains(SECRET));
    assert!(
        f.core.terminals.list(&f.thread).is_empty(),
        "nothing started"
    );
    assert!(audit(&f.core, "library.run").is_empty());
    // A wrong or extra ack is not an ack.
    let wrong = f
        .core
        .run_workflow(&w.id, &run_body(&f.thread, vec![ProfileGrant::FullAccess]))
        .await
        .unwrap_err();
    assert_eq!(code(&wrong), (409, "confirm_required"));

    let ran = f
        .core
        .run_workflow(&w.id, &run_body(&f.thread, vec![ProfileGrant::EnvValues]))
        .await
        .unwrap();
    let rows = audit(&f.core, "library.run");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].detail["grants"], json!(["env_values"]));
    assert_eq!(rows[0].detail["env_sets"], json!(["Staging"]));
    assert_eq!(rows[0].detail["terminal"], json!(ran.terminal_id));
    assert!(!serde_json::to_string(&rows).unwrap().contains(SECRET));
    assert!(
        !serde_json::to_string(&rows).unwrap().contains("printenv"),
        "never a command's text"
    );
}

#[cfg(unix)]
async fn finished_output(f: &Fixture, terminal: &str) -> String {
    let session = f.core.terminals.named(&f.thread, terminal).unwrap();
    let mut state = session.output_state();
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while *state.borrow() == crate::host::terminal::PtyOutputState::Reading {
            state.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    String::from_utf8_lossy(&session.scrollback()).into_owned()
}

#[cfg(unix)]
#[tokio::test]
async fn a_run_gets_the_value_in_its_environment_and_its_output_is_masked() {
    let f = fixture();
    let set = env_set_with_value(&f, "Staging", "API_TOKEN");
    let w = f
        .core
        .create_workflow(&wf(
            "Show",
            "printenv API_TOKEN; echo done",
            vec![],
            vec![set.id.clone()],
        ))
        .unwrap();
    let ran = f
        .core
        .run_workflow(&w.id, &run_body(&f.thread, vec![ProfileGrant::EnvValues]))
        .await
        .unwrap();
    let shown = finished_output(&f, &ran.terminal_id).await;
    assert!(!shown.contains(SECRET), "{shown}");
    assert!(shown.contains("[hidden: env API_TOKEN]"), "{shown}");
    assert!(shown.contains("done"));
}

#[cfg(unix)]
#[tokio::test]
async fn a_run_with_no_sets_or_a_local_entry_needs_no_ack_and_is_audited() {
    let f = fixture();
    let w = f
        .core
        .create_workflow(&wf("Hi", "echo hello", vec![], vec![]))
        .unwrap();
    let ran = f
        .core
        .run_workflow(&w.id, &run_body(&f.thread, vec![]))
        .await
        .unwrap();
    assert!(
        finished_output(&f, &ran.terminal_id)
            .await
            .contains("hello")
    );
    let rows = audit(&f.core, "library.run");
    assert_eq!(rows[0].detail["grants"], json!([]));
    let extra = f
        .core
        .run_workflow(&w.id, &run_body(&f.thread, vec![ProfileGrant::EnvValues]))
        .await
        .unwrap_err();
    assert_eq!(
        code(&extra),
        (409, "confirm_required"),
        "the ack must match exactly what the run needs"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_run_uses_the_apps_posix_shell_and_never_the_persons_shell() {
    let f = fixture();
    let marker = f.dir.path().join("pwned");
    // A parameter default is validated but not shell-checked: this one would run `touch` if it were not one word.
    let mut hostile = param("v", ParamKind::Text);
    let value = format!("x'; touch {} #", marker.display());
    hostile.default = Some(value.clone());
    let workflow = f
        .core
        .create_workflow(&wf(
            "Shell",
            "printf 'SHELL=%s|%s' \"$0\" {{v}}",
            vec![hostile],
            vec![],
        ))
        .unwrap();
    let ran = f
        .core
        .run_workflow(&workflow.id, &run_body(&f.thread, vec![]))
        .await
        .unwrap();
    let shown = finished_output(&f, &ran.terminal_id).await;
    // `$0` is the shell the run picked: the app's fixed `/bin/sh`, not the person's `$SHELL`. The last
    // `SHELL=` is the run's own output, after anything a login profile printed.
    let shell = shown
        .rsplit("SHELL=")
        .next()
        .unwrap_or_default()
        .split('|')
        .next()
        .unwrap_or_default()
        .trim();
    assert!(matches!(shell, "/bin/sh" | "sh"), "{shown}");
    assert!(shown.contains(&value), "{shown}");
    assert!(!marker.exists(), "the value added no command");
}

#[tokio::test]
async fn a_run_needs_one_of_thread_or_project_and_a_trusted_folder() {
    let f = fixture();
    let w = f
        .core
        .create_workflow(&wf("Hi", "echo hello", vec![], vec![]))
        .unwrap();
    let neither = RunWorkflow {
        values: BTreeMap::new(),
        thread_id: None,
        project_id: None,
        ack_grants: vec![],
    };
    assert_eq!(
        code(&f.core.run_workflow(&w.id, &neither).await.unwrap_err()),
        (400, "invalid_input")
    );
    let both = RunWorkflow {
        values: BTreeMap::new(),
        thread_id: Some(f.thread.clone()),
        project_id: Some(f.project.clone()),
        ack_grants: vec![],
    };
    assert_eq!(
        code(&f.core.run_workflow(&w.id, &both).await.unwrap_err()),
        (400, "invalid_input")
    );
    f.core.store.untrust_project_for_test(&f.project);
    assert_eq!(
        code(
            &f.core
                .run_workflow(&w.id, &run_body(&f.thread, vec![]))
                .await
                .unwrap_err()
        ),
        (403, "folder_untrusted")
    );
    assert!(f.core.terminals.list(&f.thread).is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn an_untrusted_source_asks_once_then_again_when_the_command_changes() {
    let f = fixture();
    let set = f
        .core
        .create_env_set(&EnvSetDraft {
            name: "Staging".into(),
            description: String::new(),
            keys: vec!["K".into()],
        })
        .unwrap();
    let doc = json!({
        "format": "orbyn-library", "version": 1, "exported_at": "x",
        "workflows": [{ "name": "Imported", "command": "echo one", "env_set_names": ["staging"] }]
    });
    let answer = f
        .core
        .import_library(doc.to_string().as_bytes())
        .await
        .unwrap();
    assert_eq!(answer.created, 1);
    let w = f.core.list_workflows().unwrap().remove(0);
    assert_eq!(w.source, orbyn_contracts::LibrarySource::Import);
    assert!(w.needs_ack);
    assert_eq!(w.env_set_ids, vec![set.id.clone()]);

    let error = f
        .core
        .run_workflow(&w.id, &run_body(&f.thread, vec![]))
        .await
        .unwrap_err();
    let CoreError::Refused { detail, .. } = &error else {
        panic!()
    };
    assert_eq!(
        detail["grants"],
        json!(["untrusted_source", "env_values"]),
        "both codes, together"
    );
    assert!(f.core.terminals.list(&f.thread).is_empty());
    let both = vec![ProfileGrant::UntrustedSource, ProfileGrant::EnvValues];
    f.core
        .run_workflow(&w.id, &run_body(&f.thread, both.clone()))
        .await
        .unwrap();
    let rows = audit(&f.core, "library.run");
    assert_eq!(
        rows[0].detail["grants"],
        json!(["untrusted_source", "env_values"])
    );
    assert_eq!(rows[0].detail["source"], "import");

    // The ack is on the entry: the second run needs only what is still true (the sets).
    assert!(!f.core.get_workflow(&w.id).unwrap().needs_ack);
    let again = f
        .core
        .run_workflow(&w.id, &run_body(&f.thread, vec![]))
        .await
        .unwrap_err();
    let CoreError::Refused { detail, .. } = &again else {
        panic!()
    };
    assert_eq!(detail["grants"], json!(["env_values"]));
    f.core
        .run_workflow(&w.id, &run_body(&f.thread, vec![ProfileGrant::EnvValues]))
        .await
        .unwrap();

    // Editing the command (or the parameters) takes the ack back.
    let patch = UpdateWorkflow {
        seen_revision: 1,
        name: None,
        description: None,
        command: Some("echo two".into()),
        params: None,
        env_set_ids: Some(vec![]),
    };
    f.core.update_workflow(&w.id, &patch).unwrap();
    assert!(f.core.get_workflow(&w.id).unwrap().needs_ack);
    let after = f
        .core
        .run_workflow(&w.id, &run_body(&f.thread, vec![]))
        .await
        .unwrap_err();
    let CoreError::Refused { detail, .. } = &after else {
        panic!()
    };
    assert_eq!(detail["grants"], json!(["untrusted_source"]));
    f.core
        .run_workflow(
            &w.id,
            &run_body(&f.thread, vec![ProfileGrant::UntrustedSource]),
        )
        .await
        .unwrap();
    // Renaming alone leaves it acknowledged.
    let rename = UpdateWorkflow {
        seen_revision: 2,
        name: Some("Renamed".into()),
        description: None,
        command: None,
        params: None,
        env_set_ids: None,
    };
    f.core.update_workflow(&w.id, &rename).unwrap();
    assert!(!f.core.get_workflow(&w.id).unwrap().needs_ack);
}

#[test]
fn engine_terminals_never_take_environment_values() {
    use crate::host::terminal::{PtySpec, RunSecrets, TerminalLabel};
    let f = fixture();
    let spec = PtySpec {
        program: "/bin/sh".into(),
        args: vec![],
        cwd: f.dir.path().into(),
        cols: 80,
        rows: 24,
        engine: true,
    };
    let secrets = RunSecrets {
        env: vec![],
        mask: LiteralMasker::new(&[]).unwrap(),
    };
    let error = f
        .core
        .terminals
        .open_named_with("t", "x", TerminalLabel::default(), spec, Some(secrets))
        .unwrap_err();
    assert_eq!(code(&error), (400, "invalid_input"));
}

#[test]
fn no_crew_or_autopilot_code_reaches_the_library() {
    fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                scan(&path, hits);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                if [
                    "run_workflow",
                    "env_sets",
                    "library_workflow",
                    "spawn_values",
                    "crate::library",
                ]
                .iter()
                .any(|w| text.contains(w))
                {
                    hits.push(path.display().to_string());
                }
            }
        }
    }
    let mut hits = Vec::new();
    scan(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/crew"),
        &mut hits,
    );
    assert!(
        hits.is_empty(),
        "Crew code must not touch the library: {hits:?}"
    );
}

// ---- reading committed .orbyn/ files ----

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

struct Repo {
    f: Fixture,
    root: std::path::PathBuf,
    project: String,
}

fn repo() -> Repo {
    let f = fixture();
    let root = f.dir.path().join("repo");
    std::fs::create_dir_all(root.join(".orbyn")).unwrap();
    git(&root, &["init", "-q"]);
    let project = f
        .core
        .store
        .add_project(NewProject {
            path: &root.to_string_lossy(),
            name: "r",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    f.core.store.trust_project(&project.id, "user").unwrap();
    Repo {
        root,
        project: project.id,
        f,
    }
}

impl Repo {
    fn commit(&self, files: &[(&str, &str)]) {
        for (name, text) in files {
            std::fs::write(self.root.join(".orbyn").join(name), text).unwrap();
        }
        git(&self.root, &["add", "-A"]);
        git(&self.root, &["commit", "-q", "--allow-empty", "-m", "c"]);
    }
}

fn tree_state(root: &std::path::Path) -> String {
    let out = std::process::Command::new("git")
        .args(["status", "--porcelain", "-uall"])
        .current_dir(root)
        .output()
        .unwrap();
    let mut state = String::from_utf8(out.stdout).unwrap();
    for name in ["workflows.json", "prompts.json", "env-sets.json"] {
        state
            .push_str(&std::fs::read_to_string(root.join(".orbyn").join(name)).unwrap_or_default());
    }
    state
}

#[tokio::test]
async fn suggestions_come_from_head_only_and_the_repository_is_never_written() {
    let r = repo();
    r.commit(&[(
        "workflows.json",
        &json!([{ "name": "Committed", "command": "echo hi", "extra": 1 }]).to_string(),
    )]);
    // A dirty edit and an untracked file are in the working tree, not in HEAD.
    std::fs::write(
        r.root.join(".orbyn/workflows.json"),
        json!([{ "name": "Dirty", "command": "rm -rf /" }]).to_string(),
    )
    .unwrap();
    std::fs::write(
        r.root.join(".orbyn/prompts.json"),
        json!([{ "name": "Untracked", "body": "x" }]).to_string(),
    )
    .unwrap();
    let before = tree_state(&r.root);
    let answer = r.f.core.library_suggestions(&r.project).await.unwrap();
    assert_eq!(answer.workflows.len(), 1);
    assert_eq!(answer.workflows[0].name, "Committed");
    assert_eq!(
        (answer.workflows[0].file.as_str(), answer.workflows[0].index),
        ("workflows.json", 0)
    );
    assert!(answer.prompts.is_empty() && answer.env_sets.is_empty() && answer.problems.is_empty());
    let copied =
        r.f.core
            .copy_suggestion(
                &r.project,
                &CopySuggestion {
                    file: "workflows.json".into(),
                    index: 0,
                },
            )
            .await
            .unwrap();
    assert_eq!(copied.kind, LibraryKind::Workflow);
    assert_eq!(copied.entry["name"], "Committed");
    assert_eq!(copied.entry["source"], "repo");
    assert_eq!(tree_state(&r.root), before, "the repository is unchanged");
    assert_eq!(r.f.core.list_workflows().unwrap().len(), 1);
}

#[tokio::test]
async fn a_bad_entry_is_dropped_with_its_index_and_the_rest_is_served() {
    let r = repo();
    let workflows = json!({ "version": 1, "workflows": [
        { "name": "Good", "command": "echo 1" },
        { "name": "", "command": "x" },
        { "name": "Quoted", "command": "echo \"{{p}}\"", "params": [{ "name": "p", "kind": "text" }] },
        { "name": "Also good", "command": "echo 2" }
    ]});
    let sets = json!([{ "name": "Staging", "keys": { "API_TOKEN": "hunter2-leak", "API_BASE": "" } }, { "name": "Bad", "keys": ["PATH"] }]);
    r.commit(&[
        ("workflows.json", &workflows.to_string()),
        ("env-sets.json", &sets.to_string()),
        ("prompts.json", "{ not json"),
    ]);
    let answer = r.f.core.library_suggestions(&r.project).await.unwrap();
    assert_eq!(
        answer.workflows.iter().map(|w| w.index).collect::<Vec<_>>(),
        [0, 3]
    );
    assert_eq!(answer.env_sets.len(), 1);
    assert_eq!(answer.env_sets[0].keys, ["API_BASE", "API_TOKEN"]);
    let problems: Vec<(&str, u32, &str)> = answer
        .problems
        .iter()
        .map(|p| (p.file.as_str(), p.index, p.code.as_str()))
        .collect();
    assert!(problems.contains(&("workflows.json", 1, "invalid_input")));
    assert!(problems.contains(&("workflows.json", 2, "invalid_input")));
    assert!(problems.contains(&("env-sets.json", 0, "value_dropped")));
    assert!(problems.contains(&("env-sets.json", 1, "key_refused")));
    assert!(problems.contains(&("prompts.json", 0, "not_json")));
    assert!(
        !serde_json::to_string(&answer)
            .unwrap()
            .contains("hunter2-leak"),
        "a value in the file is never shown"
    );
    let copied =
        r.f.core
            .copy_suggestion(
                &r.project,
                &CopySuggestion {
                    file: "env-sets.json".into(),
                    index: 0,
                },
            )
            .await
            .unwrap();
    assert_eq!(copied.kind, LibraryKind::EnvSet);
    assert!(
        copied.entry["keys"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| k["present"] == false)
    );
    assert_eq!(r.f.memory.accounts().len(), 0);
    let missing =
        r.f.core
            .copy_suggestion(
                &r.project,
                &CopySuggestion {
                    file: "workflows.json".into(),
                    index: 9,
                },
            )
            .await
            .unwrap_err();
    assert_eq!(code(&missing), (404, "not_found"));
    let other =
        r.f.core
            .copy_suggestion(
                &r.project,
                &CopySuggestion {
                    file: "../x.json".into(),
                    index: 0,
                },
            )
            .await
            .unwrap_err();
    assert_eq!(code(&other), (400, "invalid_input"));
}

#[tokio::test]
async fn an_oversized_file_a_symlink_an_unknown_version_and_an_untrusted_folder_are_handled() {
    let r = repo();
    let big = format!(
        "[{}]",
        vec![r#"{"name":"n","command":"x"}"#; 3000].join(",")
    );
    assert!(big.len() > SUGGESTIONS_FILE_BYTES);
    std::fs::write(r.root.join("outside.json"), "[]").unwrap();
    r.commit(&[
        ("workflows.json", &big),
        ("env-sets.json", r#"{"version":2,"env_sets":[]}"#),
    ]);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("../outside.json", r.root.join(".orbyn/prompts.json")).unwrap();
        git(&r.root, &["add", "-A"]);
        git(&r.root, &["commit", "-q", "-m", "link"]);
    }
    let answer = r.f.core.library_suggestions(&r.project).await.unwrap();
    assert!(answer.workflows.is_empty());
    let problems: Vec<(&str, &str)> = answer
        .problems
        .iter()
        .map(|p| (p.file.as_str(), p.code.as_str()))
        .collect();
    assert!(problems.contains(&("workflows.json", "too_large")));
    assert!(problems.contains(&("env-sets.json", "unknown_version")));
    #[cfg(unix)]
    assert!(problems.contains(&("prompts.json", "not_a_file")));
    r.f.core.store.untrust_project_for_test(&r.project);
    assert_eq!(
        code(&r.f.core.library_suggestions(&r.project).await.unwrap_err()),
        (403, "folder_untrusted")
    );
}

// ---- import and export ----

#[tokio::test]
async fn an_import_drops_a_bad_entry_renames_a_clash_and_never_replaces() {
    let f = fixture();
    f.core
        .create_workflow(&wf("Tests", "cargo test", vec![], vec![]))
        .unwrap();
    let doc = json!({
        "format": "orbyn-library", "version": 1, "exported_at": "x", "mystery": { "a": 1 },
        "workflows": [
            { "name": "Tests", "command": "cargo test --all", "surprise": true },
            { "name": "Bad", "command": "echo '{{x}}'" },
            { "name": "Linked", "command": "echo 1", "env_set_names": ["No such set"] }
        ],
        "prompts": [{ "name": "P", "body": "hi" }, 42],
        "env_sets": [{ "name": "Staging", "keys": ["API_BASE"] }],
        "dry_run": true
    });
    let dry = f
        .core
        .import_library(doc.to_string().as_bytes())
        .await
        .unwrap();
    assert!(dry.dry_run);
    assert_eq!((dry.created, dry.renamed, dry.refused), (3, 1, 2));
    assert_eq!(
        f.core.list_workflows().unwrap().len(),
        1,
        "a dry run writes nothing"
    );
    assert!(
        f.core.list_prompts().unwrap().is_empty() && f.core.list_env_sets().unwrap().is_empty()
    );
    let renamed = dry
        .lines
        .iter()
        .find(|l| l.kind == "workflow" && l.index == 0)
        .unwrap();
    assert_eq!(renamed.name.as_deref(), Some("Tests (2)"));
    assert!(
        dry.problems
            .iter()
            .any(|p| p.kind == "workflow" && p.index == 2 && p.code == "unknown_env_set")
    );

    let mut real = doc.clone();
    real["dry_run"] = json!(false);
    let done = f
        .core
        .import_library(real.to_string().as_bytes())
        .await
        .unwrap();
    assert_eq!(
        (done.created, done.renamed, done.refused),
        (dry.created, dry.renamed, dry.refused)
    );
    let all = f.core.list_workflows().unwrap();
    assert_eq!(
        all.iter().find(|w| w.name == "Tests").unwrap().command,
        "cargo test",
        "the existing one is untouched"
    );
    assert!(
        all.iter()
            .any(|w| w.name == "Tests (2)" && w.source == orbyn_contracts::LibrarySource::Import)
    );
    assert!(
        f.core
            .list_prompts()
            .unwrap()
            .iter()
            .all(|p| p.source == orbyn_contracts::LibrarySource::Import)
    );
    assert!(
        f.core.list_env_sets().unwrap()[0]
            .keys
            .iter()
            .all(|k| !k.present)
    );
    assert_eq!(audit(&f.core, "library.import").len(), 1);
}

#[tokio::test]
async fn an_import_is_capped_and_must_be_ours() {
    let f = fixture();
    let huge = vec![b' '; IMPORT_BYTES + 1];
    assert_eq!(
        code(&f.core.import_library(&huge).await.unwrap_err()),
        (400, "invalid_input")
    );
    assert_eq!(
        code(&f.core.import_library(b"{}").await.unwrap_err()),
        (400, "invalid_input")
    );
    assert_eq!(
        code(&f.core.import_library(b"not json").await.unwrap_err()),
        (400, "invalid_input")
    );
    let wrong_version = json!({ "format": "orbyn-library", "version": 2 });
    assert_eq!(
        code(
            &f.core
                .import_library(wrong_version.to_string().as_bytes())
                .await
                .unwrap_err()
        ),
        (400, "invalid_input")
    );
}

#[tokio::test]
async fn an_export_has_names_and_never_a_value_or_an_id() {
    let f = fixture();
    let set = env_set_with_value(&f, "Staging", "API_TOKEN");
    f.core
        .create_workflow(&wf("W", "echo 1", vec![], vec![set.id.clone()]))
        .unwrap();
    let doc = f
        .core
        .export_library(Some("workflows,env_sets"))
        .await
        .unwrap();
    let text = serde_json::to_string(&doc).unwrap();
    assert!(!text.contains(SECRET) && !text.contains(&set.id));
    assert_eq!(doc.workflows[0].env_set_names, ["Staging"]);
    assert_eq!(doc.env_sets[0].keys, ["API_TOKEN"]);
    assert!(doc.prompts.is_empty() && doc.profiles.is_empty());
    assert_eq!(
        code(&f.core.export_library(Some("secrets")).await.unwrap_err()),
        (400, "invalid_input")
    );
}
