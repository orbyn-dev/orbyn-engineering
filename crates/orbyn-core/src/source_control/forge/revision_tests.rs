use std::path::Path;

use serde_json::json;

use super::github::parse_github_revision;
use super::pulls::revision_files;
use super::*;
use crate::error::CoreError;

#[test]
fn a_github_commit_reads_its_first_line_and_login() {
    let revision = parse_github_revision(&json!({
        "sha": "abc",
        "author": { "login": "ada" },
        "commit": { "message": "Rename it\n\nBecause.", "committer": { "date": "2026-10-01T00:00:00Z" } },
    }));
    assert_eq!(revision.message, "Rename it");
    assert_eq!(revision.author.as_deref(), Some("ada"));
    assert_eq!(
        revision.committed_at.as_deref(),
        Some("2026-10-01T00:00:00Z")
    );
}

#[tokio::test]
async fn a_hash_with_anything_else_in_it_never_reaches_the_forge() {
    let refused = revision_files(Forge::GitHub, Path::new("."), "abc/../x").await;
    assert!(matches!(refused, Err(CoreError::Invalid(_))));
}
