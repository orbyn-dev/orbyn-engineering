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
