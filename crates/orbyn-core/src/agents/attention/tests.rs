//! W10: the heuristic table, the excerpt caps, the state machine with a clock handed
//! in, secret labels, and a real append whose excerpt was already redacted.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use orbyn_contracts::{
    AgUiEvent, PatternScope, ShellEvent, TerminalAttentionClearReason, TerminalAttentionEvent,
    TerminalAttentionKind,
};
use regex::Regex;
use serde_json::json;

use super::*;
use crate::app::{Core, CoreConfig};
use crate::host::terminal::{PtySpec, TerminalLabel};
use crate::redaction::Custom;
use crate::store::{NewProject, NewThread};
use crate::testing::TempDir;

const T0: i64 = 1_000_000;
const SINCE: &str = "2026-01-01T00:00:00Z";

fn evaluation(kind: TerminalAttentionKind, excerpt: &[&str]) -> Evaluation {
    Evaluation {
        kind,
        excerpt: excerpt.iter().map(|line| (*line).to_owned()).collect(),
    }
}

/// A service-made id for the state machine tests: distinct on every call, as a ULID is.
fn next_id() -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    format!("a{}", NEXT.fetch_add(1, Ordering::SeqCst))
}

fn quiet(
    registry: &mut Registry,
    thread_id: &str,
    terminal_id: &str,
    now_ms: i64,
    evaluation: Evaluation,
) -> Vec<Notice> {
    let mut make_id = next_id;
    registry.quiet(
        thread_id,
        terminal_id,
        now_ms,
        SINCE.to_owned(),
        evaluation,
        &mut make_id,
    )
}

fn waiting_id(notices: &[Notice]) -> String {
    match notices {
        [Notice::Waiting { id, .. }, ..] => id.clone(),
        other => panic!("expected a waiting notice, got {other:?}"),
    }
}

#[test]
fn the_heuristic_table() {
    use TerminalAttentionKind::{Question, YesNo};
    let cases: &[(&str, Option<TerminalAttentionKind>)] = &[
        (
            "Overwrite auth/redirect.ts with the new version? (y/N)",
            Some(YesNo),
        ),
        ("Apply these 3 changes? [Y/n]", Some(YesNo)),
        ("Delete the folder? (yes/no)", Some(YesNo)),
        ("Continue? [YES/NO]", Some(YesNo)),
        ("Continue? [y/N]:", Some(YesNo)),
        (
            "Which test command should I run to check the fix?",
            Some(Question),
        ),
        ("Password:", Some(Question)),
        ("Enter a name:", Some(Question)),
        ("Choose an option:", Some(Question)),
        ("Apply these changes?\n❯ 1. yes\n  2. no", Some(Question)),
        ("Choose a model:\n1) sonnet\n2) opus", Some(Question)),
        ("❯ 1. yes\n  2. no", Some(Question)),
        ("\u{1b}[31mDelete everything? (y/N)\u{1b}[0m", Some(YesNo)),
        ("   Compiling orbyn-core v0.1.0", None),
        ("Finished dev profile in 12.4s", None),
        ("error: could not compile `orbyn-core`", None),
        ("error: cannot find crate `x`; did you mean `y`?", None),
        ("npm ERR! code ELIFECYCLE", None),
        ("", None),
        ("\n\n   \n", None),
    ];
    for (text, want) in cases {
        assert_eq!(heuristic::classify(text), *want, "{text:?}");
    }
}

#[test]
fn ansi_controls_and_carriage_redraws_are_stripped() {
    assert_eq!(
        excerpt::plain(
            "\u{1b}[31mred\u{1b}[0m\r\n\u{1b}]8;;http://example.invalid\u{7}link\u{1b}]8;;\u{7}\u{0}\u{7}"
        ),
        "red\nlink"
    );
    assert_eq!(excerpt::plain("old\rnew\nafter\r"), "new\nafter");
    assert_eq!(excerpt::plain("fallback"), "fallback");
}

#[test]
fn an_excerpt_is_cut_not_wrapped() {
    let text = (0..20)
        .map(|at| format!("{at}: {}", "x ".repeat(150)))
        .collect::<Vec<_>>()
        .join("\n");
    let lines = excerpt::lines(&text);
    assert!(lines.len() <= excerpt::MAX_LINES, "{lines:?}");
    assert!(
        lines
            .iter()
            .all(|line| line.chars().count() <= excerpt::MAX_LINE_CHARS),
        "{lines:?}"
    );
    assert!(lines.join("\n").chars().count() <= excerpt::MAX_EXCERPT_CHARS);
    assert!(lines.last().is_some_and(|line| line.starts_with("19: ")));
}

#[test]
fn an_excerpt_leaves_room_for_the_event() {
    let text = (0..excerpt::MAX_LINES)
        .map(|_| "😀".repeat(excerpt::MAX_LINE_CHARS))
        .collect::<Vec<_>>()
        .join("\n");
    let lines = excerpt::lines(&text);
    let bytes = lines.iter().map(String::len).sum::<usize>() + lines.len().saturating_sub(1);
    assert!(bytes <= excerpt::MAX_EXCERPT_BYTES, "{bytes}");
    assert!(lines.join("\n").chars().count() <= excerpt::MAX_EXCERPT_CHARS);
}

#[test]
fn a_secret_keeps_only_the_prompt_label() {
    let empty = Redactor::default();
    let reading = evaluate(&empty, "Welcome\n\n\u{1b}[1mPassword: \u{1b}[0m", true).unwrap();
    assert_eq!(reading.kind, TerminalAttentionKind::Secret);
    assert_eq!(reading.excerpt, vec!["Password:".to_owned()]);

    let long = format!("Password: {}", "x".repeat(400));
    let reading = evaluate(&empty, &long, true).unwrap();
    assert_eq!(reading.excerpt.len(), 1);
    assert!(
        reading.excerpt[0].chars().count() <= excerpt::MAX_LABEL_CHARS,
        "{:?}",
        reading.excerpt
    );

    // A line echoed after the label is text the person typed, never the label.
    let reading = evaluate(&empty, "Password:\nwhat the person typed", true).unwrap();
    assert_eq!(reading.excerpt, vec!["Password:".to_owned()]);

    // With no prompt-like line, nothing is taken for the label.
    let reading = evaluate(&empty, "what the person typed", true).unwrap();
    assert!(reading.excerpt.is_empty(), "{:?}", reading.excerpt);

    // The mode decides: the words alone are a question when echo is on.
    let reading = evaluate(&empty, "Password:", false).unwrap();
    assert_eq!(reading.kind, TerminalAttentionKind::Question);
}

#[test]
fn one_active_item_per_terminal_updates_the_same_id_after_five_seconds() {
    let mut registry = Registry::default();
    let first = quiet(
        &mut registry,
        "t",
        "tm",
        T0,
        evaluation(TerminalAttentionKind::Question, &["Which one?"]),
    );
    let id = waiting_id(&first);

    let early = quiet(
        &mut registry,
        "t",
        "tm",
        T0 + 4_999,
        evaluation(TerminalAttentionKind::Question, &["Which two?"]),
    );
    assert!(early.is_empty(), "an update waits 5 s: {early:?}");

    let later = quiet(
        &mut registry,
        "t",
        "tm",
        T0 + 5_000,
        evaluation(TerminalAttentionKind::Question, &["Which two?"]),
    );
    assert_eq!(waiting_id(&later), id, "an update keeps the id");

    let unchanged = quiet(
        &mut registry,
        "t",
        "tm",
        T0 + 10_000,
        evaluation(TerminalAttentionKind::Question, &["Which two?"]),
    );
    assert!(unchanged.is_empty(), "nothing changed: {unchanged:?}");
}

#[test]
fn a_new_episode_needs_a_clear_and_ten_seconds() {
    let mut registry = Registry::default();
    let first = quiet(
        &mut registry,
        "t",
        "tm",
        T0,
        evaluation(TerminalAttentionKind::Question, &["First?"]),
    );
    let first_id = waiting_id(&first);

    let cleared = registry.output("t", "tm");
    assert!(matches!(
        cleared.as_slice(),
        [Notice::Clear {
            reason: TerminalAttentionClearReason::Output,
            ..
        }]
    ));

    let early = quiet(
        &mut registry,
        "t",
        "tm",
        T0 + 9_999,
        evaluation(TerminalAttentionKind::Question, &["Second?"]),
    );
    assert!(early.is_empty(), "10 s since the last waiting: {early:?}");

    let later = quiet(
        &mut registry,
        "t",
        "tm",
        T0 + 10_000,
        evaluation(TerminalAttentionKind::Question, &["Second?"]),
    );
    let second_id = waiting_id(&later);
    assert_ne!(second_id, first_id, "an id is never reused");
}

#[test]
fn every_clear_reason_is_reported_once() {
    type Clear = fn(&mut Registry, &str, &str) -> Vec<Notice>;
    let clears: [(Clear, TerminalAttentionClearReason); 3] = [
        (Registry::input, TerminalAttentionClearReason::Input),
        (Registry::output, TerminalAttentionClearReason::Output),
        (Registry::closed, TerminalAttentionClearReason::Closed),
    ];
    for (clear, reason) in clears {
        let mut registry = Registry::default();
        quiet(
            &mut registry,
            "t",
            "tm",
            T0,
            evaluation(TerminalAttentionKind::YesNo, &["Continue? (y/N)"]),
        );
        let notices = clear(&mut registry, "t", "tm");
        assert!(matches!(
            notices.as_slice(),
            [Notice::Clear { reason: found, .. }] if *found == reason
        ));
        assert!(
            clear(&mut registry, "t", "tm").is_empty(),
            "a clear for no active item changes nothing"
        );
    }
}

#[test]
fn eight_items_per_thread_drop_the_oldest() {
    let mut registry = Registry::default();
    let mut ids = Vec::new();
    for at in 0..8 {
        let notices = quiet(
            &mut registry,
            "t1",
            &format!("tm{at}"),
            T0 + at,
            evaluation(TerminalAttentionKind::Question, &["Which?"]),
        );
        ids.push(waiting_id(&notices));
    }
    let ninth = quiet(
        &mut registry,
        "t1",
        "tm8",
        T0 + 100,
        evaluation(TerminalAttentionKind::Question, &["Which?"]),
    );
    assert_eq!(ninth.len(), 2, "{ninth:?}");
    assert!(matches!(
        &ninth[0],
        Notice::Clear {
            id,
            reason: TerminalAttentionClearReason::Closed,
            ..
        } if *id == ids[0]
    ));
    assert!(matches!(&ninth[1], Notice::Waiting { .. }));
    assert_eq!(registry.active_in_thread("t1"), 8);
}

#[test]
fn thirty_two_items_across_the_service_drop_the_oldest() {
    let mut registry = Registry::default();
    let mut first_id = None;
    for at in 0..32 {
        let thread = format!("t{}", at / 8);
        let notices = quiet(
            &mut registry,
            &thread,
            &format!("tm{at}"),
            T0 + at,
            evaluation(TerminalAttentionKind::Question, &["Which?"]),
        );
        if at == 0 {
            first_id = Some(waiting_id(&notices));
        }
    }
    assert_eq!(registry.active_count(), 32);
    let over = quiet(
        &mut registry,
        "t4",
        "tm32",
        T0 + 100,
        evaluation(TerminalAttentionKind::Question, &["Which?"]),
    );
    assert_eq!(over.len(), 2, "{over:?}");
    assert!(matches!(
        &over[0],
        Notice::Clear {
            id,
            reason: TerminalAttentionClearReason::Closed,
            ..
        } if Some(id) == first_id.as_ref()
    ));
    assert_eq!(registry.active_count(), 32);
}

#[test]
fn the_settings_read_a_bounded_quiet_time() {
    assert_eq!(quiet_seconds(&json!(10)), 10);
    assert_eq!(quiet_seconds(&json!(20)), 20);
    assert_eq!(quiet_seconds(&json!(120)), 120);
    assert_eq!(quiet_seconds(&json!(9)), QUIET_DEFAULT_SECS);
    assert_eq!(quiet_seconds(&json!(121)), QUIET_DEFAULT_SECS);
    assert_eq!(quiet_seconds(&json!(20.5)), QUIET_DEFAULT_SECS);
    assert!(enabled(&json!(true)));
    assert!(enabled(&json!(null)));
    assert!(!enabled(&json!(false)));
}

#[test]
fn the_quiet_time_setting_is_stored_and_checked() {
    let dir = TempDir::new();
    let core = Core::open(CoreConfig::new(
        dir.path().join("data"),
        "http://127.0.0.1:1".into(),
    ))
    .unwrap();
    assert_eq!(
        core.preference(ENABLED_PREFERENCE, None).unwrap(),
        json!(true)
    );
    for bad in [json!(9), json!(121), json!(20.5)] {
        let values = BTreeMap::from([(QUIET_PREFERENCE.to_owned(), bad)]);
        assert!(
            core.set_preferences(None, &values).is_err(),
            "a quiet time outside 10..=120 is refused"
        );
    }
    let values = BTreeMap::from([(QUIET_PREFERENCE.to_owned(), json!(30))]);
    core.set_preferences(None, &values).unwrap();
    assert_eq!(core.preference(QUIET_PREFERENCE, None).unwrap(), json!(30));
}

#[test]
fn one_watcher_per_terminal_process() {
    let hub = AttentionHub::default();
    assert!(hub.claim("t", "tm", "first"));
    assert!(
        !hub.claim("t", "tm", "first"),
        "a second open reuses its watcher"
    );
    assert!(
        hub.claim("t", "tm", "second"),
        "a reopened terminal is a new process"
    );
    hub.release("t", "tm", "first");
    assert!(
        !hub.claim("t", "tm", "second"),
        "the old process's release does not free the new slot"
    );
    hub.release("t", "tm", "second");
    assert!(hub.claim("t", "tm", "third"));
}

#[test]
fn a_waiting_value_stays_under_four_kibibytes() {
    let text: Vec<String> = (0..excerpt::MAX_LINES)
        .map(|_| "😀".repeat(excerpt::MAX_LINE_CHARS))
        .collect();
    let notice = Notice::Waiting {
        thread_id: "t".into(),
        terminal_id: "engine-console".into(),
        id: "a1".into(),
        kind: TerminalAttentionKind::Question,
        since: SINCE.into(),
        excerpt: text,
    };
    let value = notice.value(
        &"T".repeat(excerpt::MAX_TITLE_CHARS),
        &"E".repeat(excerpt::MAX_ENGINE_CHARS),
    );
    assert!(
        value_bytes(&value) < MAX_VALUE_BYTES,
        "{} bytes",
        value_bytes(&value)
    );
    let TerminalAttentionEvent::Waiting { excerpt: lines, .. } = &value else {
        panic!("expected a waiting value")
    };
    assert!(lines.len() <= excerpt::MAX_LINES);
    assert!(
        lines
            .iter()
            .all(|line| line.chars().count() <= excerpt::MAX_LINE_CHARS)
    );
}

fn core_with_thread(dir: &TempDir) -> (Arc<Core>, String) {
    let path = dir.path().join("repo");
    std::fs::create_dir_all(&path).unwrap();
    let core = Arc::new(
        Core::open(CoreConfig::new(
            dir.path().join("data"),
            "http://127.0.0.1:1".into(),
        ))
        .unwrap(),
    );
    let project = core
        .store
        .add_project(NewProject {
            path: path.to_str().unwrap(),
            name: "repo",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    core.store.trust_project(&project.id, "tester").unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id,
            title: "t".into(),
            engine: "claude".into(),
            model: None,
            cwd: path.to_str().unwrap().to_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    (core, thread.id)
}

#[tokio::test]
async fn a_quiet_console_with_a_question_appends_the_waiting_event() {
    let dir = TempDir::new();
    let (core, thread_id) = core_with_thread(&dir);
    let spec = PtySpec {
        // A pty hands the program to `CreateProcessW` as the application name, which a bare
        // `cmd.exe` is not found by: the path is resolved first, as the twin test does.
        program: console_program_absolute(),
        args: console_args_blocking(),
        cwd: dir.path().to_owned(),
        cols: 80,
        rows: 24,
        engine: true,
    };
    core.terminals
        .open_named(
            &thread_id,
            "engine-console",
            TerminalLabel {
                title: Some("Fake console".into()),
                ..TerminalLabel::default()
            },
            spec,
        )
        .unwrap();
    let session = core.terminals.named(&thread_id, "engine-console").unwrap();
    let watch = Watch {
        thread_id: thread_id.clone(),
        terminal_id: "engine-console".into(),
        terminal_title: "Fake console".into(),
        engine: "claude".into(),
        quiet: Duration::from_millis(100),
        redactor: Arc::new(Redactor::default()),
        hub: core.attention.clone(),
        store: core.store.clone(),
        shell: core.shell.clone(),
        session,
    };
    let task = tokio::spawn(run(watch));
    let deadline = Instant::now() + Duration::from_secs(10);
    let event = loop {
        let events = core.store.events_after(&thread_id, 0, 20).unwrap();
        if let Some(event) = events.into_iter().find(
            |event| matches!(&event.event, AgUiEvent::Custom { name, .. } if name == EVENT_NAME),
        ) {
            break event;
        }
        assert!(
            Instant::now() < deadline,
            "the quiet question should become an event; scrollback: {:?}",
            core.terminals
                .named(&thread_id, "engine-console")
                .map(|session| String::from_utf8_lossy(&session.scrollback()).into_owned())
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let AgUiEvent::Custom { name, value } = &event.event else {
        panic!("expected a CUSTOM event")
    };
    assert_eq!(name, EVENT_NAME);
    assert_eq!(value["op"], "waiting");
    assert_eq!(value["kind"], "yes_no");
    assert_eq!(value["terminal_id"], "engine-console");
    let excerpt = value["excerpt"].as_array().unwrap();
    assert!(
        excerpt.iter().any(|line| line
            .as_str()
            .is_some_and(|line| line.contains("Continue? (y/N)"))),
        "{excerpt:?}"
    );
    let _ = core.terminals.close_one(&thread_id, "engine-console");
    task.abort();
}

#[cfg(unix)]
fn console_program() -> std::path::PathBuf {
    "/bin/sh".into()
}

#[cfg(windows)]
fn console_program() -> std::path::PathBuf {
    "cmd.exe".into()
}

#[tokio::test]
async fn the_redactor_runs_before_the_event_is_appended() {
    let dir = TempDir::new();
    let (core, thread_id) = core_with_thread(&dir);
    let redactor = Redactor::only(vec![Custom {
        name: "token",
        regex: Regex::new("hunter2").unwrap(),
        applies_to: PatternScope::Anywhere,
    }]);
    let reading = evaluate(&redactor, "Password:\nhunter2\nContinue? (y/N)", false).unwrap();
    let mut registry = Registry::default();
    let notices = quiet(&mut registry, &thread_id, "engine-console", T0, reading);
    let mut windows = core.shell.watch();
    for notice in &notices {
        append(&core.store, &core.shell, notice, "Claude console", "claude").unwrap();
    }
    let events = core.store.events_after(&thread_id, 0, 10).unwrap();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(
        events[0].run_id.is_none(),
        "an attention event is not a run"
    );
    let AgUiEvent::Custom { name, value } = &events[0].event else {
        panic!("expected a CUSTOM event, got {:?}", events[0].event)
    };
    assert_eq!(name.as_str(), EVENT_NAME);
    assert_eq!(value["v"], json!(1));
    assert_eq!(value["op"], "waiting");
    assert_eq!(value["kind"], "yes_no");
    let text = value.to_string();
    assert!(!text.contains("hunter2"), "{text}");
    assert!(text.contains("[hidden: token]"), "{text}");
    assert!(
        events[0].seq > 0,
        "the log assigns the sequence the shell notice names"
    );
    assert_eq!(
        windows.events.try_recv().unwrap(),
        ShellEvent::ThreadEvent {
            thread_id,
            seq: events[0].seq,
        }
    );
}

/// The setting is consulted while the detector runs, not only when the console opens
/// (#1764): turning it off before the first report keeps the item away, and turning it
/// back on reports the same question, so the console really was waiting and only the
/// setting held the item back.
#[tokio::test]
async fn turning_the_setting_off_stops_new_detection_on_an_open_console() {
    let dir = TempDir::new();
    let (core, thread_id) = core_with_thread(&dir);
    let spec = PtySpec {
        program: console_program_absolute(),
        args: console_args_blocking(),
        cwd: dir.path().to_owned(),
        cols: 80,
        rows: 24,
        engine: true,
    };
    core.terminals
        .open_named(
            &thread_id,
            "engine-console",
            TerminalLabel {
                title: Some("Fake console".into()),
                ..TerminalLabel::default()
            },
            spec,
        )
        .unwrap();
    let session = core.terminals.named(&thread_id, "engine-console").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !String::from_utf8_lossy(&session.scrollback()).contains("(y/N)") {
        assert!(
            Instant::now() < deadline,
            "the fake console should print its question"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Off before the watcher's first tick could report the question.
    set_enabled(&core, false);
    let watch = Watch {
        thread_id: thread_id.clone(),
        terminal_id: "engine-console".into(),
        terminal_title: "Fake console".into(),
        engine: "claude".into(),
        quiet: Duration::from_millis(100),
        redactor: Arc::new(Redactor::default()),
        hub: core.attention.clone(),
        store: core.store.clone(),
        shell: core.shell.clone(),
        session,
    };
    let task = tokio::spawn(run(watch));
    let off = Instant::now() + Duration::from_secs(3);
    while Instant::now() < off {
        assert_eq!(
            attention_events(&core, &thread_id),
            0,
            "the setting is off, so no item is reported"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    set_enabled(&core, true);
    let deadline = Instant::now() + Duration::from_secs(10);
    while attention_events(&core, &thread_id) == 0 {
        assert!(
            Instant::now() < deadline,
            "with the setting back on the question becomes an item"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = core.terminals.close_one(&thread_id, "engine-console");
    task.abort();
}

fn set_enabled(core: &Core, on: bool) {
    let values = BTreeMap::from([(ENABLED_PREFERENCE.to_owned(), json!(on))]);
    core.set_preferences(None, &values).unwrap();
}

/// The fake console's program as the absolute path a new pty is spawned from.
fn console_program_absolute() -> std::path::PathBuf {
    let program = console_program();
    crate::host::process::resolve(&program.to_string_lossy())
        .map_or(program, |command| command.program)
}

/// A console that prints one question and then waits inside the shell, with no
/// external program to find on `PATH`.
#[cfg(unix)]
fn console_args_blocking() -> Vec<std::ffi::OsString> {
    vec![
        "-c".into(),
        "printf 'Continue? (y/N)\\n'; read answer".into(),
    ]
}

#[cfg(windows)]
fn console_args_blocking() -> Vec<std::ffi::OsString> {
    vec![
        "/D".into(),
        "/C".into(),
        "echo Continue? (y/N) & pause >nul".into(),
    ]
}

/// How many attention items a thread's log holds.
fn attention_events(core: &Core, thread_id: &str) -> usize {
    core.store
        .events_after(thread_id, 0, 50)
        .unwrap()
        .into_iter()
        .filter(
            |event| matches!(&event.event, AgUiEvent::Custom { name, .. } if name == EVENT_NAME),
        )
        .count()
}

/// Where no echo mode can be read (Windows ConPTY), a `Password:` line is a plain question and
/// its last lines are shown. They are redacted first, so a secret printed above it never leaves.
#[test]
fn a_password_line_read_as_a_question_still_hides_secret_text() {
    let redactor = Redactor::only(vec![Custom {
        name: "token",
        regex: Regex::new("hunter2").unwrap(),
        applies_to: PatternScope::Anywhere,
    }]);
    let reading = evaluate(
        &redactor,
        "token is hunter2\nhunter2 again\nPassword:",
        false,
    )
    .unwrap();
    assert_eq!(reading.kind, TerminalAttentionKind::Question);
    let text = reading.excerpt.join("\n");
    assert!(!text.contains("hunter2"), "{text}");
    assert!(text.contains("[hidden: token]"), "{text}");
}

/// The excerpt's line cut runs after redaction, and its cap (200 characters) is shorter
/// than this person value: the cut lands inside the match in the printed line, so the
/// excerpt must carry the marker, never the value's head.
#[test]
fn an_excerpt_line_cut_inside_a_long_value_shows_no_fragment() {
    let secret = format!("ACME-{}", "Q9Z8".repeat(100));
    let other = format!("ACME-{}", "X7W6".repeat(100));
    assert!(secret.chars().count() > excerpt::MAX_LINE_CHARS);
    let redactor = Redactor::only(vec![Custom {
        name: "Acme key",
        regex: Regex::new("ACME-[A-Z0-9]{400}").unwrap(),
        applies_to: PatternScope::Anywhere,
    }]);
    // The match starts at, before and just before the cut and runs past it; a second value
    // sits wholly after. At 190 the cut falls in the marker the match became.
    for (lead, marker_shown) in [(0usize, true), (150, true), (190, false)] {
        let text = format!(
            "{}{secret} {} {other}\nContinue? (y/N)",
            "a ".repeat(lead / 2),
            "b".repeat(excerpt::MAX_LINE_CHARS + 100)
        );
        let reading = evaluate(&redactor, &text, false).unwrap();
        assert_eq!(reading.kind, TerminalAttentionKind::YesNo);
        assert_eq!(reading.excerpt.len(), 2, "{:?}", reading.excerpt);
        assert_eq!(reading.excerpt[1], "Continue? (y/N)");
        let shown = &reading.excerpt[0];
        assert!(!shown.contains("ACME"), "{shown}");
        for value in [&secret, &other] {
            let chars: Vec<char> = value.chars().collect();
            for window in chars.windows(16) {
                let window: String = window.iter().collect();
                assert!(!shown.contains(&window), "{window} survived in {shown}");
            }
        }
        assert!(shown.chars().count() <= excerpt::MAX_LINE_CHARS, "{shown}");
        assert_eq!(
            shown.matches("[hidden: ").count(),
            shown.matches(']').count(),
            "a marker was split: {shown}"
        );
        assert_eq!(
            shown.contains("[hidden: Acme key]"),
            marker_shown,
            "{lead}: {shown}"
        );
    }

    // Dropping the run a cut lands in does not drop a word before it: a value whose match
    // crosses a space only stays out of the excerpt if it was hidden before the cut.
    let spaced = format!("ACME KEY-{}", "Q9Z8".repeat(100));
    let spaced_redactor = Redactor::only(vec![Custom {
        name: "Acme key",
        regex: Regex::new("ACME KEY-[A-Z0-9]{400}").unwrap(),
        applies_to: PatternScope::Anywhere,
    }]);
    for (lead, marker_shown) in [(0usize, true), (150, true), (190, false)] {
        let text = format!(
            "{}{spaced} {}\nContinue? (y/N)",
            "a ".repeat(lead / 2),
            "b".repeat(excerpt::MAX_LINE_CHARS + 100)
        );
        let reading = evaluate(&spaced_redactor, &text, false).unwrap();
        assert_eq!(reading.kind, TerminalAttentionKind::YesNo);
        let shown = &reading.excerpt[0];
        assert!(!shown.contains("ACME"), "{shown}");
        let chars: Vec<char> = spaced.chars().collect();
        for window in chars.windows(16) {
            let window: String = window.iter().collect();
            assert!(!shown.contains(&window), "{window} survived in {shown}");
        }
        assert!(shown.chars().count() <= excerpt::MAX_LINE_CHARS, "{shown}");
        assert_eq!(
            shown.matches("[hidden: ").count(),
            shown.matches(']').count(),
            "a marker was split: {shown}"
        );
        assert_eq!(
            shown.contains("[hidden: Acme key]"),
            marker_shown,
            "{lead}: {shown}"
        );
    }
}

#[test]
fn open_items_list_what_waits_in_one_thread() {
    let mut registry = Registry::default();
    let open = waiting_id(&quiet(
        &mut registry,
        "t1",
        "a",
        T0,
        evaluation(TerminalAttentionKind::YesNo, &["Apply? (y/N)"]),
    ));
    let cleared = waiting_id(&quiet(
        &mut registry,
        "t1",
        "b",
        T0 + 1,
        evaluation(TerminalAttentionKind::Question, &["Which one?"]),
    ));
    quiet(
        &mut registry,
        "t2",
        "c",
        T0 + 2,
        evaluation(TerminalAttentionKind::Question, &["Other thread?"]),
    );
    quiet(
        &mut registry,
        "t1",
        "d",
        T0 + 3,
        evaluation(TerminalAttentionKind::Secret, &["Password:"]),
    );
    registry.output("t1", "b");

    let items = registry.open_items("t1");
    let ids: Vec<&str> = items
        .iter()
        .map(|notice| match notice {
            Notice::Waiting { id, .. } => id.as_str(),
            Notice::Clear { .. } => panic!("a clear is never an open item"),
        })
        .collect();
    assert_eq!(ids.len(), 2, "{items:?}");
    assert_eq!(ids[0], open, "oldest first");
    assert!(!ids.contains(&cleared.as_str()));
    assert!(registry.open_items("t3").is_empty());
    // A secret carries only the label it was stored with.
    assert!(matches!(
        &items[1],
        Notice::Waiting { kind: TerminalAttentionKind::Secret, excerpt, .. }
            if excerpt == &["Password:".to_owned()]
    ));
}

#[tokio::test]
async fn the_open_read_names_the_terminal_and_engine_and_refuses_an_unknown_thread() {
    let dir = TempDir::new();
    let (core, thread_id) = core_with_thread(&dir);
    let reading = evaluation(TerminalAttentionKind::YesNo, &["Delete it? (y/N)"]);
    let notices = core
        .attention
        .quiet(&thread_id, "engine-console", T0, SINCE.to_owned(), reading);
    let id = waiting_id(&notices);
    let items = core.terminal_attention(&thread_id).unwrap();
    assert_eq!(items.len(), 1);
    assert!(matches!(
        &items[0],
        TerminalAttentionEvent::Waiting { v: 1, id: got, terminal_id, engine, kind, since, excerpt, .. }
            if *got == id
                && terminal_id == "engine-console"
                && engine == "claude"
                && *kind == TerminalAttentionKind::YesNo
                && since == SINCE
                && excerpt == &["Delete it? (y/N)".to_owned()]
    ));
    core.attention.closed(&thread_id, "engine-console");
    assert!(core.terminal_attention(&thread_id).unwrap().is_empty());
    assert!(core.terminal_attention("no-such-thread").is_err());
}
