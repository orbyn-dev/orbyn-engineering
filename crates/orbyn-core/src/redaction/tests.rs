//! Detectors, the person's patterns, the limits, the per-project switch and the invariants that
//! no setting can move. Secret-looking values are built at run time from parts and named by
//! label: no literal in this file stands for a secret, and no message prints one.

use std::collections::BTreeSet;

use orbyn_contracts::{
    PatternKind, PatternScope, RedactionPatternInput, RedactionPatternTestInput,
    RedactionSurfaceChange,
};
use serde_json::json;

use super::*;
use crate::app::Core;
use crate::crew::{find_secret, redact, refuse_secret};
use crate::error::CoreError;
use crate::store::NewProject;
use crate::testing::TempDir;
use crate::{CoreConfig, redaction::Surface};

const ACTOR: &str = "local";

/// One value per recognised format, built from parts.
fn sample(id: &str) -> String {
    let run = |part: &str, n: usize| part.repeat(n);
    match id {
        "private_key" => format!("-----BEGIN {} PRIVATE KEY-----", "RSA"),
        "api_key" => format!("{}-{}", "sk", run("a1", 12)),
        "github_token" => format!("{}_{}", "ghp", run("b2", 20)),
        "aws_access_key" => format!("{}A{}", "AKI", run("Q", 16)),
        "aws_secret_key" => format!("aws_secret_access_key={}", run("x9", 20)),
        "slack_token" => format!("{}-{}", "xoxb", run("1234567890", 2)),
        "bearer_token" => format!("{} {}", "Bearer", run("tok", 10)),
        "jwt" => format!(
            "{}{}.{}.{}",
            "eyJ",
            run("a", 12),
            run("h", 12),
            run("s", 12)
        ),
        "named_value" => format!("{}={}", "password", run("zq", 6)),
        other => panic!("no sample for {other}"),
    }
}

fn core(dir: &TempDir) -> Core {
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    Core::open(config).unwrap()
}

fn input(name: &str, pattern: &str, kind: PatternKind) -> RedactionPatternInput {
    RedactionPatternInput {
        name: name.into(),
        pattern: pattern.into(),
        kind,
        applies_to: PatternScope::Anywhere,
        accept_broad: None,
    }
}

fn project(core: &Core, name: &str) -> String {
    core.store
        .add_project(NewProject {
            path: &format!("/{name}"),
            name,
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap()
        .id
}

fn refused(error: CoreError) -> (&'static str, String) {
    match error {
        CoreError::PatternRefused { code, message, .. } => (code, message),
        other => panic!("expected a refused pattern, got code {}", other.code()),
    }
}

#[test]
fn every_recognised_format_is_found_and_masked() {
    let all = Redactor::locked();
    for builtin in BUILTINS {
        let value = sample(builtin.id);
        assert!(
            all.class_of(&value).is_some(),
            "{} is not found",
            builtin.id
        );
        let hidden = all.labelled(&format!("before {value} after"));
        assert!(!hidden.contains(&value), "{} survives", builtin.id);
        assert!(hidden.starts_with("before [hidden: "), "{}", builtin.id);
        assert!(
            hidden.ends_with(" after") || hidden.contains("] after"),
            "{}",
            builtin.id
        );
    }
    assert!(all.class_of("Add a parse_duration function.").is_none());
    assert!(
        all.labelled("nothing to hide here")
            .eq("nothing to hide here")
    );
}

#[test]
fn a_format_turned_off_is_no_longer_hidden_and_the_others_still_are() {
    let off = BTreeSet::from(["slack_token".to_owned()]);
    let redactor = Redactor::with(&off, Vec::new());
    let slack = sample("slack_token");
    let github = sample("github_token");
    assert_eq!(redactor.labelled(&slack), slack);
    assert!(!redactor.labelled(&github).contains(&github));
}

#[test]
fn overlapping_matches_are_hidden_once_under_the_first_label() {
    let token = sample("github_token");
    let text = format!("token = {token}");
    let hidden = Redactor::locked().labelled(&text);
    assert_eq!(
        hidden.matches("[hidden:").count(),
        1,
        "one mask for one value"
    );
    assert!(!hidden.contains(&token));
}

#[test]
fn dots_replace_each_hidden_character_and_labels_name_the_format() {
    let value = sample("aws_access_key");
    let dotted = Redactor::locked().dotted(&format!("id {value}"));
    assert_eq!(dotted, format!("id {}", "•".repeat(value.chars().count())));
    assert!(
        Redactor::locked()
            .labelled(&value)
            .contains("[hidden: AWS access key]")
    );
}

#[test]
fn crews_scan_and_redaction_use_every_format_and_are_unchanged_in_output() {
    for builtin in BUILTINS {
        let value = sample(builtin.id);
        let object = json!({ "outcome": format!("use {value} to log in") });
        assert_eq!(find_secret(&object), Some(builtin.class), "{}", builtin.id);
        assert!(refuse_secret(&object).is_err(), "{}", builtin.id);
        let shown = redact(&format!("use {value} now"));
        assert!(
            shown.contains("[redacted]") && !shown.contains(&value),
            "{}",
            builtin.id
        );
    }
    assert_eq!(redact("plain text"), "plain text");
}

#[test]
fn a_text_pattern_is_literal_and_star_stands_for_a_run_of_non_spaces() {
    let regex = compile(PatternKind::Text, "*.corp.example.com").unwrap();
    assert!(regex.is_match("db1.corp.example.com is up"));
    assert!(!regex.is_match("corpXexampleXcom"), "a dot is a dot");
    let plain = compile(PatternKind::Text, "a+b(c)").unwrap();
    assert!(plain.is_match("x a+b(c) y"), "regex characters are literal");
}

#[test]
fn a_regular_expression_that_is_not_valid_says_where_in_plain_words() {
    let found = compile(PatternKind::Regex, "ab_([A-Za-z0-9]{24}").unwrap_err();
    assert!(
        found.message.starts_with(
            "This isn’t a valid pattern: the group opened at position 3 is not closed"
        )
    );
    assert_eq!(found.position, Some(3));
    for (pattern, word) in [
        ("[abc", "is not closed"),
        ("a)", "closing bracket"),
        ("*a", "nothing to repeat"),
        ("(?=x)", "look-around"),
        ("a\\", "lone backslash"),
    ] {
        let found = compile(PatternKind::Regex, pattern).unwrap_err();
        assert!(found.message.contains(word), "{pattern}: {}", found.message);
    }
}

#[test]
fn limits_bound_the_size_the_program_and_the_nesting_of_a_pattern() {
    assert!(compile(PatternKind::Regex, "").is_err());
    assert!(compile(PatternKind::Regex, "   ").is_err());
    let long = "a".repeat(MAX_PATTERN_CHARS + 1);
    assert!(
        compile(PatternKind::Regex, &long)
            .unwrap_err()
            .message
            .contains("at most")
    );
    assert!(compile(PatternKind::Regex, &"a".repeat(MAX_PATTERN_CHARS)).is_ok());
    // A repeat of a repeat blows up the compiled program, however short the pattern.
    let big = compile(PatternKind::Regex, r"(\w{100}){100}").unwrap_err();
    assert!(big.message.contains("too complex"), "{}", big.message);
    let deep = format!("{}a{}", "(".repeat(40), ")".repeat(40));
    assert!(compile(PatternKind::Regex, &deep).is_err());
    assert!(
        compile(PatternKind::Text, "**")
            .unwrap_err()
            .message
            .contains("at least 3")
    );
    assert!(compile(PatternKind::Text, "a*b").is_err());
}

#[test]
fn a_nested_repeat_that_would_backtrack_forever_elsewhere_finishes_here() {
    // The classic runaway shape. The engine has no backtracking, so it is linear in the text.
    let regex = compile(PatternKind::Regex, r"(a+)+$").unwrap();
    let text = format!("{}!", "a".repeat(20_000));
    let started = std::time::Instant::now();
    assert!(!regex.is_match(&text));
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn empty_matches_hide_nothing_and_never_litter_the_text() {
    let regex = compile(PatternKind::Regex, "x*").unwrap();
    let only = Redactor::only(vec![Custom {
        name: "empty",
        regex,
        applies_to: PatternScope::Anywhere,
    }]);
    assert_eq!(only.labelled("abc"), "abc");
    assert_eq!(only.labelled("axxb"), "a[hidden: empty]b");
}

#[test]
fn a_pattern_for_environment_values_looks_only_inside_assignments() {
    let regex = compile(PatternKind::Regex, "corp[0-9]+").unwrap();
    let env = Redactor::only(vec![Custom {
        name: "host",
        regex,
        applies_to: PatternScope::EnvironmentValues,
    }]);
    assert_eq!(env.labelled("HOST=corp42"), "HOST=[hidden: host]");
    assert_eq!(
        env.labelled("export HOST=\"a.corp7\""),
        "export HOST=\"a.[hidden: host]\""
    );
    assert_eq!(
        env.labelled("ping corp42"),
        "ping corp42",
        "prose is left alone"
    );
}

#[test]
fn each_masked_example_matches_no_detector() {
    for builtin in BUILTINS {
        assert!(
            Redactor::locked().spans(builtin.example).is_empty(),
            "the example of {} would be hidden",
            builtin.id
        );
        assert!(
            Redactor::locked().class_of(builtin.example).is_none(),
            "{}",
            builtin.id
        );
    }
}

#[test]
fn text_over_the_window_is_read_whole_and_a_value_on_the_edge_is_still_hidden() {
    let token = sample("github_token");
    let filler = "x ".repeat(custom::MAX_APPLY_BYTES / 2);
    // The value straddles the first window's end.
    let cut = custom::MAX_APPLY_BYTES - token.len() / 2;
    let mut text = filler.clone() + &filler;
    text.replace_range(cut..cut + token.len(), &token);
    assert!(text.len() > custom::MAX_APPLY_BYTES);
    let hidden = Redactor::locked().labelled(&text);
    assert!(!hidden.contains(&token));
    assert_eq!(hidden.matches("[hidden:").count(), 1);
}

#[test]
fn the_compiled_set_is_kept_per_settings_revision() {
    let dir = TempDir::new();
    let core = core(&dir);
    let one = core.redactor(Surface::Agents, None).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &one,
        &core.redactor(Surface::Agents, None).unwrap()
    ));
    core.change_redaction_format(ACTOR, "jwt", false).unwrap();
    let two = core.redactor(Surface::Agents, None).unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(&one, &two),
        "a new revision builds a new set"
    );
}

#[test]
fn the_list_never_carries_pattern_text_and_the_edit_dialog_reads_one() {
    let dir = TempDir::new();
    let core = core(&dir);
    let text = format!("{}_[A-Za-z0-9]{{8}}", "acme");
    let view = core
        .save_redaction_pattern(ACTOR, None, &input("Acme key", &text, PatternKind::Regex))
        .unwrap();
    assert!(!serde_json::to_string(&view).unwrap().contains(&text));
    let detail = core.redaction_pattern(&view.patterns[0].id).unwrap();
    assert_eq!(detail.pattern, text);
    assert!(matches!(
        core.redaction_pattern("nope"),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn an_error_never_echoes_the_pattern() {
    let marker = format!("{}{}", "uniq", "marker");
    for pattern in [
        format!("{marker}(?P<n"),
        format!("{marker}(?P<n>a)(?P<n>b)"),
        format!("{marker}[z-a]"),
    ] {
        let found = compile(PatternKind::Regex, &pattern).unwrap_err();
        assert!(!found.message.contains(&marker), "{}", found.message);
    }
}

#[test]
fn an_edit_that_could_stop_a_pattern_matching_widens_and_a_rename_or_add_does_not() {
    let mk = |id: &str, name: &str, pattern: &str| StoredPattern {
        id: id.into(),
        name: name.into(),
        pattern: pattern.into(),
        kind: PatternKind::Regex,
        applies_to: PatternScope::Anywhere,
    };
    let before = Stored {
        patterns: vec![mk("a", "A", "abc+")],
        ..Stored::default()
    };
    let with = |patterns: Vec<StoredPattern>| Stored {
        patterns,
        ..before.clone()
    };
    assert!(!with(vec![mk("a", "Renamed", "abc+")]).widens(&before));
    assert!(!with(vec![mk("a", "A", "abc+"), mk("b", "B", "xyz")]).widens(&before));
    assert!(
        with(vec![mk("a", "A", "abcd")]).widens(&before),
        "a changed pattern"
    );
    assert!(with(vec![]).widens(&before), "a removed one");
    let mut scope = mk("a", "A", "abc+");
    scope.applies_to = PatternScope::EnvironmentValues;
    assert!(with(vec![scope]).widens(&before), "a narrower place");
    let swapped = with(vec![mk("b", "A", "abc+")]);
    assert!(swapped.widens(&before), "a different id is a removal");
}

#[test]
fn the_person_patterns_alone_mask_display_text() {
    let dir = TempDir::new();
    let core = core(&dir);
    let text = format!("{}_[a-z]{{6}}", "corp");
    core.save_redaction_pattern(ACTOR, None, &input("Corp id", &text, PatternKind::Regex))
        .unwrap();
    let shown = core.display_redactor().unwrap();
    let github = sample("github_token");
    assert_eq!(
        shown.labelled(&format!("see corp_abcdef and {github}")),
        format!("see [hidden: Corp id] and {github}")
    );
}

#[test]
fn the_view_lists_every_format_on_and_the_ledger_locked() {
    let dir = TempDir::new();
    let core = core(&dir);
    let view = core.redaction_settings().unwrap();
    assert!(view.terminal && view.agents && view.crew_locked);
    assert_eq!(view.formats.len(), BUILTINS.len());
    assert!(view.formats.iter().all(|format| format.enabled));
    assert!(view.patterns.is_empty() && view.projects_off.is_empty());
}

#[test]
fn turning_a_format_off_and_on_changes_what_agents_read_but_never_crews_rule() {
    let dir = TempDir::new();
    let core = core(&dir);
    let slack = sample("slack_token");
    let view = core
        .change_redaction_format(ACTOR, "slack_token", false)
        .unwrap();
    assert!(
        !view
            .formats
            .iter()
            .find(|f| f.id == "slack_token")
            .unwrap()
            .enabled
    );
    let agents = core.redactor(Surface::Agents, None).unwrap();
    assert_eq!(agents.labelled(&slack), slack, "shown to the agent");
    let terminal = core.redactor(Surface::Terminal, None).unwrap();
    assert_eq!(terminal.dotted(&slack), slack);
    // The invariant: Crew still finds and removes it.
    assert_eq!(
        find_secret(&json!({ "outcome": slack })),
        Some("slack token")
    );
    assert!(!redact(&slack).contains(&slack));
    core.change_redaction_format(ACTOR, "slack_token", true)
        .unwrap();
    let agents = core.redactor(Surface::Agents, None).unwrap();
    assert_ne!(agents.labelled(&slack), slack);
    assert!(matches!(
        core.change_redaction_format(ACTOR, "no_such_format", false),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn the_two_places_switch_alone_and_nothing_switches_crew() {
    let dir = TempDir::new();
    let core = core(&dir);
    let value = sample("github_token");
    core.change_redaction_surfaces(
        ACTOR,
        &RedactionSurfaceChange {
            terminal: Some(false),
            agents: None,
        },
    )
    .unwrap();
    assert!(core.redactor(Surface::Terminal, None).unwrap().is_empty());
    assert!(!core.redactor(Surface::Agents, None).unwrap().is_empty());
    core.change_redaction_surfaces(
        ACTOR,
        &RedactionSurfaceChange {
            terminal: Some(true),
            agents: Some(false),
        },
    )
    .unwrap();
    assert!(core.redactor(Surface::Agents, None).unwrap().is_empty());
    let view = core.redaction_settings().unwrap();
    assert!(view.terminal && !view.agents && view.crew_locked);
    assert!(find_secret(&json!({ "outcome": value })).is_some());
}

#[test]
fn redaction_off_for_one_project_leaves_the_others_and_the_terminal_alone() {
    let dir = TempDir::new();
    let core = core(&dir);
    let one = project(&core, "one");
    let two = project(&core, "two");
    let value = sample("jwt");
    let view = core.change_redaction_project(ACTOR, &one, false).unwrap();
    assert_eq!(view.projects_off.len(), 1);
    assert_eq!(view.projects_off[0].name, "one");
    let seen = |id: &str| {
        core.redactor(Surface::Agents, Some(id))
            .unwrap()
            .labelled(&value)
    };
    assert_eq!(seen(&one), value);
    assert_ne!(seen(&two), value);
    assert!(!core.redactor(Surface::Terminal, None).unwrap().is_empty());
    assert!(
        find_secret(&json!({ "outcome": value })).is_some(),
        "Crew ignores it"
    );
    let view = core.change_redaction_project(ACTOR, &one, true).unwrap();
    assert!(view.projects_off.is_empty());
    assert!(matches!(
        core.change_redaction_project(ACTOR, "nope", false),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn a_person_pattern_is_saved_listed_applied_edited_and_removed() {
    let dir = TempDir::new();
    let core = core(&dir);
    let pattern = format!("{}_[A-Za-z0-9]{{8}}", "acme");
    let view = core
        .save_redaction_pattern(
            ACTOR,
            None,
            &input("Acme key", &pattern, PatternKind::Regex),
        )
        .unwrap();
    let id = view.patterns[0].id.clone();
    let value = format!("{}_{}", "acme", "Qw3Rt5Yu");
    let agents = core.redactor(Surface::Agents, None).unwrap();
    assert_eq!(
        agents.labelled(&format!("k {value}")),
        "k [hidden: Acme key]"
    );
    let renamed = core
        .save_redaction_pattern(
            ACTOR,
            Some(&id),
            &input("Acme token", &pattern, PatternKind::Regex),
        )
        .unwrap();
    assert_eq!(renamed.patterns.len(), 1);
    assert_eq!(renamed.patterns[0].name, "Acme token");
    let view = core.delete_redaction_pattern(ACTOR, &id).unwrap();
    assert!(view.patterns.is_empty());
    assert!(matches!(
        core.delete_redaction_pattern(ACTOR, &id),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn a_person_pattern_is_not_part_of_crews_rule() {
    let dir = TempDir::new();
    let core = core(&dir);
    let pattern = format!("{}_[A-Za-z0-9]{{8}}", "acme");
    core.save_redaction_pattern(
        ACTOR,
        None,
        &input("Acme key", &pattern, PatternKind::Regex),
    )
    .unwrap();
    let value = format!("{}_{}", "acme", "Qw3Rt5Yu");
    assert!(find_secret(&json!({ "outcome": value })).is_none());
}

#[test]
fn an_invalid_a_long_a_duplicate_and_an_overfull_pattern_are_refused_with_a_reason() {
    let dir = TempDir::new();
    let core = core(&dir);
    let bad = core
        .save_redaction_pattern(
            ACTOR,
            None,
            &input("Bad", "ab_([A-Za-z]{2}", PatternKind::Regex),
        )
        .unwrap_err();
    assert_eq!(refused(bad).0, "invalid_pattern");
    let nameless = core
        .save_redaction_pattern(ACTOR, None, &input("  ", "abcdef", PatternKind::Text))
        .unwrap_err();
    assert!(refused(nameless).1.contains("name"));
    core.save_redaction_pattern(ACTOR, None, &input("One", "abcdef", PatternKind::Text))
        .unwrap();
    let again = core
        .save_redaction_pattern(ACTOR, None, &input("one", "ghijkl", PatternKind::Text))
        .unwrap_err();
    assert!(refused(again).1.contains("already"));
    for n in 1..MAX_PATTERNS {
        core.save_redaction_pattern(
            ACTOR,
            None,
            &input(&format!("P{n}"), &format!("lit{n}x"), PatternKind::Text),
        )
        .unwrap();
    }
    let full = core
        .save_redaction_pattern(ACTOR, None, &input("Last", "lastone", PatternKind::Text))
        .unwrap_err();
    assert!(refused(full).1.contains("at most"));
    assert_eq!(
        core.redaction_settings().unwrap().patterns.len(),
        MAX_PATTERNS
    );
}

#[test]
fn a_pattern_that_hides_ordinary_text_needs_a_second_yes() {
    let dir = TempDir::new();
    let core = core(&dir);
    let (code, message) = refused(
        core.save_redaction_pattern(ACTOR, None, &input("Everything", ".*", PatternKind::Regex))
            .unwrap_err(),
    );
    assert_eq!(code, "pattern_too_broad");
    assert!(message.contains("very broad"));
    assert!(core.redaction_settings().unwrap().patterns.is_empty());
    let mut again = input("Everything", ".*", PatternKind::Regex);
    again.accept_broad = Some(true);
    assert_eq!(
        core.save_redaction_pattern(ACTOR, None, &again)
            .unwrap()
            .patterns
            .len(),
        1
    );
}

#[test]
fn trying_a_pattern_reports_matches_masks_the_sample_and_measures_recent_lines() {
    let dir = TempDir::new();
    let core = core(&dir);
    let sample = format!("STRIPE_KEY={}_{}", "live", "Abcdefgh12");
    let tried = core.test_redaction_pattern(&RedactionPatternTestInput {
        pattern: format!("{}_[A-Za-z0-9]{{10}}", "live"),
        kind: PatternKind::Regex,
        applies_to: PatternScope::Anywhere,
        sample: sample.clone(),
        lines: None,
    });
    assert!(tried.problem.is_none() && !tried.broad);
    assert_eq!(tried.matches.len(), 1);
    assert_eq!((tried.matches[0].start, tried.matches[0].end), (11, 26));
    assert_eq!(tried.masked, format!("STRIPE_KEY={}", "•".repeat(15)));
    let lines: Vec<String> = (0..40).map(|n| format!("line {n}")).collect();
    let broad = core.test_redaction_pattern(&RedactionPatternTestInput {
        pattern: "line".into(),
        kind: PatternKind::Text,
        applies_to: PatternScope::Anywhere,
        sample: "line one".into(),
        lines: Some(lines),
    });
    assert_eq!((broad.lines_matched, broad.lines_total), (40, 40));
    assert!(broad.broad);
    // Too few lines say nothing: the ordinary text is used instead.
    let few = core.test_redaction_pattern(&RedactionPatternTestInput {
        pattern: "line".into(),
        kind: PatternKind::Text,
        applies_to: PatternScope::Anywhere,
        sample: String::new(),
        lines: Some(vec!["line".into(); 3]),
    });
    assert!(few.lines_total > 3 && !few.broad);
}

#[test]
fn trying_an_invalid_pattern_returns_the_problem_and_nothing_else() {
    let dir = TempDir::new();
    let core = core(&dir);
    let tried = core.test_redaction_pattern(&RedactionPatternTestInput {
        pattern: "ab_([A-Za-z0-9]{24}".into(),
        kind: PatternKind::Regex,
        applies_to: PatternScope::Anywhere,
        sample: "abc".into(),
        lines: None,
    });
    let problem = tried.problem.unwrap();
    assert_eq!(problem.position, Some(3));
    assert!(tried.matches.is_empty() && tried.masked.is_empty() && !tried.broad);
}

#[test]
fn text_tried_is_cut_to_the_limits() {
    let dir = TempDir::new();
    let core = core(&dir);
    let tried = core.test_redaction_pattern(&RedactionPatternTestInput {
        pattern: "zzz".into(),
        kind: PatternKind::Text,
        applies_to: PatternScope::Anywhere,
        sample: "z".repeat(MAX_SAMPLE_CHARS * 2),
        lines: Some(vec!["zzz".into(); MAX_CONTEXT_LINES * 2]),
    });
    assert_eq!(tried.lines_total as usize, MAX_CONTEXT_LINES);
    assert!(tried.masked.chars().count() <= MAX_SAMPLE_CHARS);
}

#[test]
fn an_unreadable_or_unknown_document_falls_back_to_hiding_everything() {
    assert_eq!(Stored::parse(Some("not json")), Stored::default());
    assert_eq!(Stored::parse(None), Stored::default());
    let partial = Stored::parse(Some(r#"{"terminal":false}"#));
    assert!(
        !partial.terminal && partial.agents,
        "a missing field is the safe default"
    );
    let ghost = Stored::parse(Some(r#"{"formats_off":["slack_token","gone_format"]}"#));
    assert_eq!(
        ghost.known_formats_off(),
        BTreeSet::from(["slack_token".to_owned()])
    );
    let broken = Stored::parse(Some(
        r#"{"patterns":[{"id":"x","name":"n","pattern":"(","kind":"regex","applies_to":"anywhere"}]}"#,
    ));
    let kept = broken.redactor(Surface::Agents, None);
    assert!(
        !kept.is_empty(),
        "a pattern that no longer compiles is skipped, the formats stay"
    );
}

#[test]
fn changes_that_widen_are_audited_by_name_without_the_pattern_text() {
    let dir = TempDir::new();
    let core = core(&dir);
    core.change_redaction_format(ACTOR, "jwt", false).unwrap();
    core.change_redaction_format(ACTOR, "jwt", true).unwrap();
    let text = format!("{}_[a-z]{{12}}", "zeta");
    core.save_redaction_pattern(ACTOR, None, &input("Zeta", &text, PatternKind::Regex))
        .unwrap();
    let rows = core.store.list_audit(None, 50).unwrap();
    let ours: Vec<_> = rows
        .iter()
        .filter(|row| row.action.starts_with("redaction."))
        .collect();
    assert_eq!(ours.len(), 3);
    assert!(ours.iter().any(|row| row.detail["widens"] == true));
    assert!(
        ours.iter()
            .all(|row| !row.detail.to_string().contains(&text))
    );
}

#[test]
fn the_settings_are_not_in_the_account_sync_allowlist() {
    use crate::account::settings_record::{SYNCED_KEYS, is_synced_key};
    assert!(!is_synced_key("redaction"));
    assert!(
        SYNCED_KEYS
            .iter()
            .all(|key| !key.contains("redact") && !key.contains("secret"))
    );
    let dir = TempDir::new();
    let core = core(&dir);
    core.change_redaction_format(ACTOR, "jwt", false).unwrap();
    // Stored apart from the preferences and the server settings, the only documents synced.
    let globals = core.store.settings_snapshot().unwrap().globals;
    assert!(globals.keys().all(|key| !key.contains("redaction")));
    assert!(
        core.store.redaction_json().unwrap().is_some(),
        "and it is stored"
    );
}
