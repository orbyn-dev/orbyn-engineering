//! The risk tier of a diff: paths, size and the revert rate of the area and the engine, each with its reason.

use crate::crew::{DiffFacts, Rate, Risk, area_of, areas_of, score};

fn facts(paths: &[&str], files: u32, lines: u32) -> DiffFacts {
    DiffFacts {
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        files,
        lines,
        ..DiffFacts::default()
    }
}

fn risk(paths: &[&str]) -> Risk {
    score(&facts(paths, u32::try_from(paths.len()).unwrap(), 10)).risk
}

#[test]
fn docs_and_tests_that_are_small_are_low_and_ordinary_work_is_medium() {
    assert_eq!(risk(&["docs/crew.md"]), Risk::Low);
    assert_eq!(risk(&["README.md", "crates/core/tests/a.rs"]), Risk::Low);
    assert_eq!(risk(&["crates/core/src/lib.rs"]), Risk::Medium);
    assert_eq!(risk(&["crates/core/src/lib.rs", "docs/a.md"]), Risk::Medium);
    // Big docs are not free.
    assert_eq!(score(&facts(&["docs/a.md"], 1, 500)).risk, Risk::High);
    assert_eq!(score(&facts(&["docs/a.md"], 1, 150)).risk, Risk::Medium);
}

#[test]
fn auth_keys_crypto_migrations_ci_and_dependencies_are_high_with_a_reason() {
    for path in [
        "src/auth/login.rs",
        "crates/core/src/crypto.rs",
        "src/session_token.ts",
        "migrations/001_init.sql",
        "crates/core/src/store/migrations.rs",
        ".github/workflows/ci.yml",
        "Cargo.toml",
        "web/package-lock.json",
        ".cargo/config.toml",
    ] {
        let scored = score(&facts(&[path], 1, 10));
        assert_eq!(scored.risk, Risk::High, "a sensitive path");
        assert!(!scored.reasons.is_empty());
    }
    // A word, not letters: author is not auth, and a test of auth is a test.
    assert_eq!(risk(&["src/author.rs"]), Risk::Medium);
    assert_eq!(risk(&["src/app/config.toml"]), Risk::Medium);
    assert_eq!(risk(&["crates/core/tests/auth_flow.rs"]), Risk::Low);
}

#[test]
fn a_large_diff_is_high() {
    assert_eq!(score(&facts(&["src/a.rs"], 21, 10)).risk, Risk::High);
    assert_eq!(score(&facts(&["src/a.rs"], 3, 401)).risk, Risk::High);
    assert_eq!(score(&facts(&["src/a.rs"], 20, 400)).risk, Risk::Medium);
}

#[test]
fn a_high_revert_rate_makes_an_area_or_an_engine_high_only_with_enough_merges() {
    let mut f = facts(&["crates/core/src/a.rs"], 1, 10);
    f.areas = vec![(
        "crates/core".into(),
        Rate {
            merges: 10,
            reverts: 2,
        },
    )];
    let scored = score(&f);
    assert_eq!(scored.risk, Risk::High);
    assert!(scored.reasons[0].contains("reverted in 2 of its last 10"));
    // Too little data changes nothing, however bad the rate.
    f.areas = vec![(
        "crates/core".into(),
        Rate {
            merges: 9,
            reverts: 9,
        },
    )];
    assert_eq!(score(&f).risk, Risk::Medium);
    f.areas = vec![(
        "crates/core".into(),
        Rate {
            merges: 50,
            reverts: 9,
        },
    )];
    assert_eq!(score(&f).risk, Risk::Medium, "18 percent is under the line");
    f.engine = Some((
        "alpha".into(),
        Rate {
            merges: 20,
            reverts: 5,
        },
    ));
    assert_eq!(score(&f).risk, Risk::High);
    assert!(
        score(&f)
            .not_measured
            .iter()
            .any(|n| n.contains("coverage"))
    );
}

#[test]
fn an_area_is_the_first_two_folders() {
    assert_eq!(area_of("crates/orbyn-core/src/a.rs"), "crates/orbyn-core");
    assert_eq!(area_of("docs/crew.md"), "docs/crew.md");
    assert_eq!(area_of("README.md"), "README.md");
    assert_eq!(
        areas_of(&["a/b/c".into(), "a/b/d".into(), "x".into()]),
        ["a/b", "x"]
    );
}
