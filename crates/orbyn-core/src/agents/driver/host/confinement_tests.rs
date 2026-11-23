use std::ffi::OsString;

use super::*;
use crate::agents::driver::host::support::{confined_spec, run_spec};

fn vars(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)))
        .collect()
}

#[test]
fn the_allowlist_keeps_a_login_home_and_path_and_adds_the_identity() {
    let identity = vec![("GIT_AUTHOR_NAME".to_owned(), "worker".to_owned())];
    let env = allowlist(
        &vars(&[("HOME", "/Users/p"), ("PATH", "/usr/bin"), ("LANG", "C")]),
        false,
        &identity,
        "thread-9",
    );
    assert_eq!(env["HOME"], "/Users/p");
    assert_eq!(env["LANG"], "C");
    assert!(env["PATH"].contains("/usr/bin"));
    assert_eq!(env["GIT_AUTHOR_NAME"], "worker");
    assert_eq!(env["ORBYN_WORKER_ID"], "thread-9");
}

#[test]
fn no_credential_and_no_data_folder_reach_a_frame() {
    let env = allowlist(
        &vars(&[
            ("HOME", "/Users/p"),
            ("ANTHROPIC_API_KEY", "sk-secret"),
            ("GITHUB_TOKEN", "ghp-secret"),
            ("AWS_SECRET_ACCESS_KEY", "aws-secret"),
            ("ORBYN_DATA_DIR", "/data"),
            ("ORBYN_VAULT_KEY", "vault"),
            ("DB_PASSWORD", "pw"),
        ]),
        false,
        &[],
        "w",
    );
    let text = serde_json::to_string(&env).unwrap();
    for leaked in [
        "sk-secret",
        "ghp-secret",
        "aws-secret",
        "vault",
        "/data",
        "\"pw\"",
    ] {
        assert!(!text.contains(leaked), "{leaked} leaked into the allowlist");
    }
    assert_eq!(env["HOME"], "/Users/p");
}

#[test]
fn an_unrestricted_run_has_no_confinement_frame() {
    assert_eq!(build(&run_spec(), "claude").unwrap(), None);
}

#[cfg(target_os = "macos")]
#[test]
fn a_confined_run_gets_the_tool_prefix_the_environment_and_the_roots() {
    if let Some(reason) = crate::host::confine::unavailable() {
        eprintln!("skipped: restrictions are unavailable here ({reason})");
        return;
    }
    let (_dir, spec) = confined_spec();
    let frame = build(&spec, "claude").unwrap().unwrap();
    let prefix = frame["argvPrefix"].as_array().unwrap();
    assert_eq!(prefix[0], "/usr/bin/sandbox-exec");
    assert!(
        prefix
            .iter()
            .all(|part| part != "orbyn-confinement-placeholder")
    );
    assert_eq!(frame["env"]["ORBYN_WORKER_ID"], "worker-thread");
    assert_eq!(frame["cwd"], spec.cwd.to_string_lossy().as_ref());
    assert!(
        frame["writeRoots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|root| root == spec.checkout.to_string_lossy().as_ref())
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn where_nothing_can_restrict_a_confined_run_is_refused_not_started_bare() {
    let (_dir, spec) = confined_spec();
    let error = build(&spec, "claude").unwrap_err();
    assert!(
        matches!(&error, CoreError::EngineUnavailable(message)
            if Some(message.as_str()) == crate::host::confine::unavailable().as_deref()),
        "refused with the platform's own reason: {error:?}"
    );
}
