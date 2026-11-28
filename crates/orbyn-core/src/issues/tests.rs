use super::*;

#[test]
fn a_github_remote_is_read_in_either_form_and_nothing_else_is() {
    for (url, repo) in [
        ("https://github.com/acme/app.git", Some("acme/app")),
        ("https://github.com/acme/app", Some("acme/app")),
        ("https://user@github.com/acme/app.git/", Some("acme/app")),
        ("git@github.com:acme/app.git", Some("acme/app")),
        ("ssh://git@github.com/acme/app.git", Some("acme/app")),
        ("https://gitlab.com/acme/app.git", None),
        ("https://github.com.evil.com/acme/app", None),
        ("https://github.com/acme", None),
        ("https://github.com/acme/app/extra", None),
        ("/local/path", None),
    ] {
        assert_eq!(github_repo_of_remote(url).as_deref(), repo, "{url}");
    }
}

#[test]
fn a_keychain_failure_never_carries_a_token() {
    for error in [
        CredentialError::InvalidKey,
        CredentialError::Unavailable,
        CredentialError::Blocked,
        CredentialError::Failed,
    ] {
        let message = credential_error(error).public_message();
        assert!(!message.to_lowercase().contains("glpat"), "{message}");
    }
}

/// One scope's pages are the only ones a write drops: a scope whose name merely starts with the written one is
/// another scope and keeps what it cached (docs/issues.md, "Cache").
#[test]
fn a_write_drops_only_the_scope_whose_name_it_names() {
    let query = IssueQuery::default();
    let written = IssueDesk::list_key(TrackerKind::AzureDevops, "org/Project", &query);
    let longer = IssueDesk::list_key(TrackerKind::AzureDevops, "org/Project Two", &query);
    let mut listings = Listings::default();
    let dropped = listings.dropped;
    for key in [&written, &longer] {
        listings.put_since(
            dropped,
            key.clone(),
            ScopeList {
                issues: Vec::new(),
                counts: None,
                next: None,
            },
        );
    }
    assert_eq!(listings.entries.len(), 2, "both scopes start cached");

    listings.drop_prefixed(&IssueDesk::scope_prefix(
        TrackerKind::AzureDevops,
        "org/Project",
    ));
    assert!(
        !listings.entries.contains_key(&written),
        "the written scope's page is dropped"
    );
    assert!(
        listings.entries.contains_key(&longer),
        "a scope whose name starts with the written one keeps its pages"
    );
}
