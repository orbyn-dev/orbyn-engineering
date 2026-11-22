//! `http_probe` on a loopback server the test starts itself: the status the URL answers is the
//! check, and a probe that got no answer is an error rather than a pass.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use orbyn_contracts::{CheckOutcome, CheckParams, CrewCheck};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::crew::checks::{Site, run_check};
use crate::crew::http_probe::probe;

const OK: &str = "HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
const NOT_FOUND: &str = "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
const MOVED: &str = "HTTP/1.1 302 Found\r\nlocation: /probe\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";

fn check(url: &str, expect_status: u16) -> CrewCheck {
    serde_json::from_value(json!({
        "id": "chk-probe",
        "kind": "http_probe",
        "proves": "the page answers with its status",
        "params": { "url": url, "expect_status": expect_status }
    }))
    .unwrap()
}

fn site_of(root: &std::path::Path) -> Site<'_> {
    Site {
        worktree: root,
        deadline: Instant::now() + Duration::from_secs(60),
        base: None,
    }
}

/// A loopback server that answers one request per response in `answers`, in order, and counts
/// them. The listener stays open while answers remain, so nothing else is answered.
async fn serve(answers: &'static [&'static str]) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/probe", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&hits);
    tokio::spawn(async move {
        for answer in answers {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            // Read the whole request head first, so closing after the answer sends no reset.
            let mut head = Vec::new();
            let mut chunk = [0u8; 512];
            while head.len() < 4096 {
                let read = stream.read(&mut chunk).await.unwrap_or(0);
                if read == 0 {
                    break;
                }
                head.extend_from_slice(&chunk[..read]);
                if head.windows(4).any(|end| end == b"\r\n\r\n") {
                    break;
                }
            }
            seen.fetch_add(1, Ordering::SeqCst);
            let _ = stream.write_all(answer.as_bytes()).await;
            let _ = stream.flush().await;
        }
    });
    (url, hits)
}

#[tokio::test]
async fn the_expected_status_passes() {
    let (url, hits) = serve(&[OK]).await;
    let dir = crate::testing::TempDir::new();
    let ran = run_check(&check(&url, 200), &site_of(dir.path())).await;
    assert_eq!(ran.outcome, CheckOutcome::Pass, "{ran:?}");
    assert!(ran.log.contains("answered 200"), "{ran:?}");
    assert!(ran.reason.is_none(), "{ran:?}");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn another_status_is_a_finding_that_names_the_one_observed() {
    let (url, hits) = serve(&[NOT_FOUND]).await;
    let dir = crate::testing::TempDir::new();
    let ran = run_check(&check(&url, 200), &site_of(dir.path())).await;
    assert_eq!(ran.outcome, CheckOutcome::Fail, "{ran:?}");
    let reason = ran.reason.unwrap_or_default();
    assert!(reason.contains("expected status 200"), "{reason}");
    assert!(reason.contains("answered 404"), "{reason}");
    assert_eq!(ran.names, ["status: 404"], "{ran:?}");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_redirect_is_compared_and_never_followed() {
    // A followed redirect would land on the second answer and pass. The 302 itself is the answer.
    let (url, hits) = serve(&[MOVED, OK]).await;
    let dir = crate::testing::TempDir::new();
    let ran = run_check(&check(&url, 200), &site_of(dir.path())).await;
    assert_eq!(ran.outcome, CheckOutcome::Fail, "{ran:?}");
    assert_eq!(ran.names, ["status: 302"], "{ran:?}");
    assert_eq!(hits.load(Ordering::SeqCst), 1, "a redirect was followed");
}

#[tokio::test]
async fn a_probe_with_no_answer_is_an_error_never_a_pass() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let dir = crate::testing::TempDir::new();
    let url = format!("http://127.0.0.1:{port}/probe");
    let ran = run_check(&check(&url, 200), &site_of(dir.path())).await;
    assert_eq!(ran.outcome, CheckOutcome::Error, "{ran:?}");
    assert!(ran.log.contains("refused"), "{ran:?}");
    let reason = ran.reason.unwrap_or_default();
    assert!(reason.contains("could not reach the URL"), "{reason}");
    assert!(!reason.contains(&url), "the key's URL reached a worker");
}

#[tokio::test]
async fn a_probe_that_never_answers_times_out_as_an_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/probe", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        // Held without an answer: the probe's own budget is what ends the wait.
        tokio::time::sleep(Duration::from_secs(30)).await;
        drop(stream);
    });
    let params = match check(&url, 200).typed_params().unwrap() {
        CheckParams::HttpProbe(params) => params,
        other => panic!("unexpected params: {other:?}"),
    };
    let started = Instant::now();
    let ran = probe(
        &params,
        Duration::from_millis(250),
        "http_probe: chk-timeout".to_owned(),
        started,
    )
    .await;
    assert_eq!(ran.outcome, CheckOutcome::Error, "{ran:?}");
    let reason = ran.reason.unwrap_or_default();
    assert!(reason.contains("ran out of time"), "{reason}");
    assert!(started.elapsed() < Duration::from_secs(10), "{ran:?}");
}
