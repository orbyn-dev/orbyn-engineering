//! The real client is built only for a loopback IP literal under the `LocalOnly` gate, and it
//! follows no redirect. Both are the gate's own promise (docs/account.md, "Engine pin"): a name
//! can be made to point anywhere, and a redirect could carry a session to another host.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;
use crate::account::AccountBackendKind;
use crate::account::engine::{CONNECTED, connect};

fn connected() -> usize {
    CONNECTED.with(std::cell::Cell::get)
}

fn center(dir: &TempDir, name: &str, address: &str) -> crate::account::AccountCenter {
    crate::account::AccountCenter::new(
        AccountGate::LocalOnly,
        AccountBackendKind::Engine {
            address: address.to_owned(),
        },
        Arc::new(crate::store::Store::open(&dir.path().join(name), "t").unwrap()),
        dir.path().join(format!("{name}.kb.json")),
        Arc::new(std::sync::Mutex::new(())),
    )
}

/// Every address that is not a loopback IP literal. The port is never the reason for a refusal.
const REFUSED: [&str; 14] = [
    "https://api.orbyn.dev",
    "https://api.orbyn.dev:443",
    "http://api.orbyn.dev",
    "http://localhost:1",
    "https://localhost",
    "http://LOCALHOST:1",
    "http://10.0.0.1:8080",
    "http://172.16.0.1:8080",
    "http://192.168.1.2",
    "http://[fe80::1]:1",
    "http://[fd00::1]:1",
    "http://127.0.0.1.example.com:1",
    "http://0.0.0.0:1",
    "nonsense",
];

const ACCEPTED: [&str; 4] = [
    "http://127.0.0.1:9",
    "http://127.1.2.3:9",
    "http://[::1]:9",
    "https://127.0.0.1:9",
];

#[test]
fn only_a_loopback_ip_literal_builds_the_client_under_the_local_only_gate() {
    let dir = TempDir::new();
    for (i, refused) in REFUSED.iter().enumerate() {
        let before = connected();
        assert!(connect(refused, "This computer").is_err(), "{refused}");
        let center = center(&dir, &format!("refused-{i}.db"), refused);
        assert!(!center.available(), "{refused} gave a backend");
        assert_eq!(connected(), before, "{refused} built a client");
    }
    for (i, accepted) in ACCEPTED.iter().enumerate() {
        let before = connected();
        let center = center(&dir, &format!("accepted-{i}.db"), accepted);
        assert!(center.available(), "{accepted}");
        assert_eq!(connected(), before + 1, "{accepted}");
    }
}

/// A listener that only counts the connections it accepts.
async fn counting_listener() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            seen.fetch_add(1, Ordering::SeqCst);
            let _ = socket
                .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                .await;
        }
    });
    (address, count)
}

/// A server that answers every request with `status` and a `Location` to `target`; the count is
/// how many requests it got.
async fn redirecting_listener(status: &'static str, target: String) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            seen.fetch_add(1, Ordering::SeqCst);
            let target = target.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                let _ = socket.read(&mut buffer).await;
                let reply = format!(
                    "HTTP/1.1 {status}\r\nLocation: {target}/elsewhere\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = socket.write_all(reply.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    (address, count)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_adapter_never_connects_to_the_target_of_a_redirect() {
    let (target, contacted) = counting_listener().await;
    for status in [
        "301 Moved Permanently",
        "302 Found",
        "307 Temporary Redirect",
        "308 Permanent Redirect",
    ] {
        let (first, asked) = redirecting_listener(status, target.clone()).await;
        let backend = connect(&first, "This computer").unwrap();
        // A call that sends a request; whatever the answer maps to, it must be an error and the
        // redirect must not be followed.
        let result = backend.account.check_invite("a-code").await;
        assert!(result.is_err(), "{status}: a redirect is not a success");
        assert_eq!(asked.load(Ordering::SeqCst), 1, "{status}: one request");
    }
    // Give a client that did follow time to connect, then check that none did.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        contacted.load(Ordering::SeqCst),
        0,
        "the redirect target was contacted"
    );
}
