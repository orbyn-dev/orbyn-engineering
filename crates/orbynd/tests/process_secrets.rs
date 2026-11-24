//! The service token and the person key are minted by the service and handed to the shell on stdout. They must never be in
//! the service's own command line or environment, where any process of the same user can read them (`ps`, `/proc`). The
//! shell starts the service with no secret, so this holds by construction today; the test pins it against a regression that
//! would pass either one in as an argument or a variable. It reads the real process, so it runs where `/proc` exists.

#![cfg(target_os = "linux")]

mod support;

use support::{Options, Service, TempDir};

fn proc_bytes(pid: u32, file: &str) -> Vec<u8> {
    std::fs::read(format!("/proc/{pid}/{file}")).unwrap_or_else(|error| panic!("{file}: {error}"))
}

fn holds(haystack: &[u8], needle: &str) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
}

#[tokio::test]
async fn the_token_and_the_person_key_are_not_in_the_services_argv_or_environment() {
    let dir = TempDir::new();
    let service = Service::start(
        &dir.path().join("data"),
        Options {
            args: vec!["--parent-stdin".into()],
            env: vec![],
        },
    );
    let key = service.person_key.clone().expect("a shell has a key");
    assert!(service.token.len() >= 16, "a real token, not a placeholder");
    assert!(key.len() >= 16, "a real key, not a placeholder");

    let pid = service.child.id();
    for file in ["cmdline", "environ"] {
        let bytes = proc_bytes(pid, file);
        assert!(!bytes.is_empty() || file == "environ", "{file} was read");
        assert!(!holds(&bytes, &service.token), "the token is in {file}");
        assert!(!holds(&bytes, &key), "the person key is in {file}");
        if let Some(access) = &service.access_key {
            assert!(!holds(&bytes, access), "the access key is in {file}");
        }
    }
    // The control: the same read finds a value that is there, so an empty read cannot pass for a clean one.
    let cmdline = proc_bytes(pid, "cmdline");
    assert!(holds(&cmdline, "--parent-stdin"));
    service.stop().await;
}
