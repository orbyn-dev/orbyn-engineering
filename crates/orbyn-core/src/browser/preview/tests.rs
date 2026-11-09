use crate::browser::preview_bridge::Bridge;
use crate::governance::computer::cdp::{BrowserSession, LaunchOptions};
use crate::util::new_id;
use orbyn_contracts::PreviewRecordingOptions;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::crash::{CRASH_WINDOW, Crashes};
use super::favicon::favicon_url;
use super::pump::{decode_base64, pump};
use super::servers::{in_folder, parse_lsof, parse_netstat, parse_windows_listeners};
use super::*;

fn fixture_tab(session: BrowserSession, control: &str) -> Arc<Tab> {
    let (frames, _) = broadcast::channel(8);
    let (pointers, _) = broadcast::channel(16);
    Arc::new(Tab {
        info: Mutex::new(
            serde_json::from_value(json!({
                "id": "tab", "thread_id": "thread", "url": "http://fixture/",
                "control": control, "recording": false, "viewers": 0,
                "width": 100, "height": 100, "can_go_back": false,
                "can_go_forward": false, "title": "", "zoom": 1.0,
                "appearance": "system", "mobile": false, "private": false,
                "profile": null, "opened_by": "person", "crashed": false
            }))
            .unwrap(),
        ),
        session: tokio::sync::Mutex::new(session),
        frames,
        initial_frame_pending: AtomicBool::new(false),
        pointers,
        trace: Mutex::new(crate::browser::preview_trace::Trace::default()),
        recording: Mutex::new(None),
        stop: CancellationToken::new(),
        // No real browser or profile exists in this protocol fixture.
        profile: PathBuf::new(),
        keep_profile: true,
        browser: PathBuf::new(),
        options: LaunchOptions::default(),
        bridge: Mutex::new(Bridge::default()),
        files: PathBuf::new(),
    })
}

async fn fixture_session(
    safe_focus: bool,
) -> (BrowserSession, tokio::task::JoinHandle<Vec<Value>>) {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let mut requests = Vec::new();
        while let Some(Ok(frame)) = socket.next().await {
            let Ok(text) = frame.to_text() else { continue };
            let request: Value = serde_json::from_str(text).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => {
                    json!({ "targetInfos": [{ "type": "page", "targetId": "target" }] })
                }
                "Target.attachToTarget" => json!({ "sessionId": "session" }),
                "Page.getFrameTree" => json!({ "frameTree": { "frame": { "id": "main" } } }),
                "Page.createIsolatedWorld" => json!({ "executionContextId": 7 }),
                "Runtime.evaluate" => json!({ "result": { "value": safe_focus } }),
                _ => json!({}),
            };
            requests.push(request.clone());
            if socket
                .send(Message::text(
                    json!({ "id": request["id"], "result": result }).to_string(),
                ))
                .await
                .is_err()
            {
                break;
            }
        }
        requests
    });
    let session = BrowserSession::connect(&format!("ws://{address}"), None)
        .await
        .unwrap();
    (session, server)
}

#[tokio::test]
async fn unsafe_focus_suppresses_saved_labels_without_suppressing_human_dispatch() {
    let (session, server) = fixture_session(false).await;
    let tab = fixture_tab(session, "human");
    let manager = PreviewManager::default();
    lock(&manager.tabs).insert("tab".into(), tab.clone());
    let folder = std::env::temp_dir().join(format!("recording-unsafe-{}", new_id()));
    manager
        .start_recording(
            "thread",
            "tab",
            folder.clone(),
            PreviewRecordingOptions {
                show_key_presses: true,
                ..PreviewRecordingOptions::default()
            },
        )
        .unwrap();
    let owner = lock(&tab.recording).clone().unwrap();
    let input = serde_json::from_value(json!({ "kind": "key", "text": "Backspace" })).unwrap();
    manager.input("thread", "tab", &input).await.unwrap();
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(200, 100)
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    owner.frame(owner.capture(jpeg.into_inner())).await;
    let done = manager.stop_recording("thread", "tab").await.unwrap();
    assert_eq!(done.frames, 1);
    let pixels = image::load_from_memory_with_format(
        &std::fs::read(folder.join("000000.jpg")).unwrap(),
        image::ImageFormat::Jpeg,
    )
    .unwrap()
    .to_rgb8();
    for y in 62..92 {
        for x in 8..178 {
            assert!(
                pixels.get_pixel(x, y)[0] < 70,
                "unsafe focus must not produce key-badge pixels"
            );
        }
    }
    drop(owner);
    drop(tab);
    drop(manager);
    let requests = server.await.unwrap();
    let dispatched: Vec<_> = requests
        .iter()
        .filter(|request| request["method"] == "Input.dispatchKeyEvent")
        .map(|request| request["params"]["type"].as_str().unwrap())
        .collect();
    assert_eq!(dispatched, ["keyDown", "keyUp"]);
    let world = requests
        .iter()
        .find(|request| request["method"] == "Page.createIsolatedWorld")
        .expect("the focus check makes its own world");
    assert_eq!(world["params"]["frameId"], "main");
    let focus = requests
        .iter()
        .find(|request| request["method"] == "Runtime.evaluate")
        .unwrap();
    assert_eq!(
        focus["params"]["contextId"], 7,
        "the page's own scripts never see the focus check"
    );
    let _ = std::fs::remove_dir_all(folder);
}

#[tokio::test]
async fn a_dialog_never_waits_behind_the_per_frame_focus_check() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (checking, checked) = tokio::sync::oneshot::channel();
    let mut checking = Some(checking);
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let mut evaluated = 0;
        while let Some(Ok(frame)) = socket.next().await {
            let Ok(text) = frame.to_text() else { continue };
            let request: Value = serde_json::from_str(text).unwrap();
            let event = |method: &str| {
                Message::text(
                    json!({ "method": method, "sessionId": "session", "params": {} }).to_string(),
                )
            };
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => {
                    json!({ "targetInfos": [{ "type": "page", "targetId": "target" }] })
                }
                "Target.attachToTarget" => json!({ "sessionId": "session" }),
                "Page.getFrameTree" => json!({ "frameTree": { "frame": { "id": "main" } } }),
                "Page.captureScreenshot" => json!({ "data": "" }),
                "Page.createIsolatedWorld" => json!({ "executionContextId": 7 }),
                "Runtime.evaluate" => {
                    evaluated += 1;
                    if evaluated == 1 {
                        // The page opens an alert; a page script never answers now.
                        socket
                            .send(event("Page.javascriptDialogOpening"))
                            .await
                            .unwrap();
                        if let Some(checking) = checking.take() {
                            let _ = checking.send(());
                        }
                        continue;
                    }
                    json!({ "result": { "value": true } })
                }
                "Page.handleJavaScriptDialog" => {
                    socket
                        .send(event("Page.javascriptDialogClosed"))
                        .await
                        .unwrap();
                    json!({})
                }
                _ => json!({}),
            };
            if socket
                .send(Message::text(
                    json!({ "id": request["id"], "result": result }).to_string(),
                ))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let session = BrowserSession::connect(&format!("ws://{address}"), None)
        .await
        .unwrap();
    let tab = fixture_tab(session, "human");
    let manager = PreviewManager::default();
    lock(&manager.tabs).insert("tab".into(), tab.clone());
    let folder = std::env::temp_dir().join(format!("recording-dialog-{}", new_id()));
    manager
        .start_recording(
            "thread",
            "tab",
            folder.clone(),
            PreviewRecordingOptions {
                show_key_presses: true,
                ..PreviewRecordingOptions::default()
            },
        )
        .unwrap();
    let pumping = tokio::spawn(pump(tab.clone()));
    // The pump is in its focus check, which the open alert would hold.
    tokio::time::timeout(Duration::from_secs(3), checked)
        .await
        .unwrap()
        .unwrap();
    let answered = tokio::time::timeout(
        Duration::from_secs(3),
        manager.answer_dialog("thread", "tab", true, None),
    )
    .await;
    pumping.abort();
    let _ = pumping.await;
    let _ = manager.stop_recording("thread", "tab").await;
    drop(tab);
    drop(manager);
    server.abort();
    let _ = std::fs::remove_dir_all(folder);
    assert!(
        answered.is_ok(),
        "answering the dialog waited behind the focus check"
    );
    assert!(answered.unwrap().unwrap());
}

#[tokio::test]
async fn failed_summary_publication_keeps_the_manager_owner_for_retry() {
    let (session, server) = fixture_session(false).await;
    let tab = fixture_tab(session, "human");
    let manager = PreviewManager::default();
    lock(&manager.tabs).insert("tab".into(), tab.clone());
    let folder = std::env::temp_dir().join(format!("recording-retry-{}", new_id()));
    manager
        .start_recording(
            "thread",
            "tab",
            folder.clone(),
            PreviewRecordingOptions::default(),
        )
        .unwrap();
    let owner = lock(&tab.recording).clone().unwrap();
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(200, 100)
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    owner.frame(owner.capture(jpeg.into_inner())).await;
    // A directory at the final manifest path forces rename failure on both OSes.
    std::fs::create_dir(folder.join("recording.json")).unwrap();
    assert!(manager.stop_recording("thread", "tab").await.is_err());
    let retained = lock(&tab.recording).clone().unwrap();
    assert!(Arc::ptr_eq(&retained, &owner));
    assert!(!retained.active());
    assert!(tab.view().recording_error.unwrap().contains("publish"));
    assert!(
        manager
            .start_recording(
                "thread",
                "tab",
                folder.with_extension("replacement"),
                PreviewRecordingOptions::default()
            )
            .is_err()
    );
    // Move only the synthetic obstacle within this test's unique artifact folder.
    std::fs::rename(
        folder.join("recording.json"),
        folder.join("publication-obstacle"),
    )
    .unwrap();
    let done = manager.stop_recording("thread", "tab").await.unwrap();
    assert_eq!(done.frames, 1);
    assert_eq!(PathBuf::from(done.path), folder);
    assert!(lock(&tab.recording).is_none());
    let summary: Value =
        serde_json::from_slice(&std::fs::read(folder.join("recording.json")).unwrap()).unwrap();
    assert_eq!(summary["frames"], 1);
    assert!(folder.join("000000.jpg").is_file());
    assert!(!folder.join("000001.jpg").exists());
    drop(retained);
    drop(owner);
    drop(tab);
    drop(manager);
    server.await.unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

#[tokio::test]
async fn same_count_viewer_turnover_receives_the_static_frame() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (capturing, mut captures) =
        tokio::sync::mpsc::channel::<tokio::sync::oneshot::Sender<()>>(1);
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        while let Some(Ok(frame)) = socket.next().await {
            let Ok(text) = frame.to_text() else { continue };
            let request: Value = serde_json::from_str(text).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => {
                    json!({"targetInfos":[{"type":"page","targetId":"target"}]})
                }
                "Target.attachToTarget" => json!({"sessionId":"session"}),
                "Page.getFrameTree" => json!({"frameTree":{"frame":{"id":"main"}}}),
                "Page.captureScreenshot" => {
                    let (release, released) = tokio::sync::oneshot::channel();
                    if capturing.send(release).await.is_err() || released.await.is_err() {
                        break;
                    }
                    json!({"data":"static-frame"})
                }
                _ => json!({}),
            };
            if socket
                .send(Message::text(
                    json!({"id":request["id"],"result":result}).to_string(),
                ))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let session = BrowserSession::connect(&format!("ws://{address}"), None)
        .await
        .unwrap();
    let tab = fixture_tab(session, "none");
    let manager = PreviewManager::default();
    lock(&manager.tabs).insert("tab".into(), tab.clone());
    let mut first = manager.subscribe("thread", "tab").unwrap();
    let pumping = tokio::spawn(pump(tab.clone()));

    let result = tokio::time::timeout(Duration::from_secs(5), async {
        captures.recv().await.unwrap().send(()).unwrap();
        assert_eq!(first.recv().await.unwrap().as_ref(), "static-frame");

        // The next capture has already sampled one viewer. Replace that
        // viewer before its reply, so the pump never observes a count change.
        let second = captures.recv().await.unwrap();
        drop(first);
        let mut replacement = manager.subscribe("thread", "tab").unwrap();
        assert_eq!(tab.frames.receiver_count(), 1);
        second.send(()).unwrap();

        // Observing each later screenshot proves the preceding pump iteration
        // finished. No sleep guesses whether the same-count frames were sent.
        captures.recv().await.unwrap().send(()).unwrap();
        let fourth = captures.recv().await.unwrap();
        let same_count = replacement.try_recv();

        // A true count increase must still deliver the identical frame. The
        // fourth capture sampled one viewer, so release it and then the fifth.
        let mut additional = manager.subscribe("thread", "tab").unwrap();
        assert_eq!(tab.frames.receiver_count(), 2);
        fourth.send(()).unwrap();
        captures.recv().await.unwrap().send(()).unwrap();
        let recovered = replacement.recv().await.unwrap();
        let increased = additional.recv().await.unwrap();

        // The additional subscription was made during capture. Once its
        // initial frame is delivered, stable viewers suppress duplicates.
        let sixth = captures.recv().await.unwrap();
        assert!(matches!(
            replacement.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert!(matches!(
            additional.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        sixth.send(()).unwrap();
        let _seventh = captures.recv().await.unwrap();
        assert!(matches!(
            replacement.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        assert!(matches!(
            additional.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        (same_count, recovered, increased)
    })
    .await;

    // Always release the synthetic owners before asserting the regression.
    pumping.abort();
    let _ = pumping.await;
    drop(captures);
    drop(manager);
    drop(tab);
    server.await.unwrap();
    let (same_count, recovered, increased) = result.expect("controlled pump must progress");
    assert_eq!(recovered.as_ref(), "static-frame");
    assert_eq!(increased.as_ref(), "static-frame");
    assert_eq!(
        same_count
            .expect("replacement viewer must receive the unchanged page")
            .as_ref(),
        "static-frame",
    );
}

#[tokio::test]
async fn screenshot_error_releases_the_session_for_health_check_and_next_frame() {
    use futures_util::{SinkExt, StreamExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio_tungstenite::tungstenite::Message;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let shots = Arc::new(AtomicUsize::new(0));
    let checks = Arc::new(AtomicUsize::new(0));
    let (frames_seen, health_seen) = (shots.clone(), checks.clone());
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        while let Some(Ok(frame)) = socket.next().await {
            let Ok(text) = frame.to_text() else { continue };
            let request: Value = serde_json::from_str(text).unwrap();
            let method = request["method"].as_str().unwrap();
            let result = match method {
                "Target.getTargets" => {
                    json!({ "targetInfos": [{ "type": "page", "targetId": "target" }] })
                }
                "Target.attachToTarget" => json!({ "sessionId": "session" }),
                "Page.getFrameTree" => json!({ "frameTree": { "frame": { "id": "main" } } }),
                "Page.captureScreenshot" => {
                    if frames_seen.fetch_add(1, Ordering::SeqCst) == 0 {
                        socket.send(Message::text(json!({ "id": request["id"], "error": { "message": "synthetic screenshot refusal" } }).to_string())).await.unwrap();
                        continue;
                    }
                    json!({ "data": "synthetic-frame" })
                }
                "Runtime.evaluate" => {
                    health_seen.fetch_add(1, Ordering::SeqCst);
                    json!({ "result": { "value": true } })
                }
                _ => json!({}),
            };
            socket
                .send(Message::text(
                    json!({ "id": request["id"], "result": result }).to_string(),
                ))
                .await
                .unwrap();
        }
    });
    let session = BrowserSession::connect(&format!("ws://{address}"), None)
        .await
        .unwrap();
    let tab = fixture_tab(session, "none");
    let mut viewer = tab.frames.subscribe();
    let pumping = tokio::spawn(pump(tab.clone()));
    let next = tokio::time::timeout(Duration::from_secs(2), viewer.recv()).await;
    // Reap the synthetic tasks even when the regression fails on the old path.
    pumping.abort();
    let _ = pumping.await;
    drop(tab);
    server.await.unwrap();
    assert!(
        next.is_ok(),
        "first screenshot refusal must recover, not keep its session lock: shots={}, health={}",
        shots.load(Ordering::SeqCst),
        checks.load(Ordering::SeqCst)
    );
    assert_eq!(next.unwrap().unwrap().as_ref(), "synthetic-frame");
    assert_eq!(shots.load(Ordering::SeqCst), 2);
    assert_eq!(checks.load(Ordering::SeqCst), 1);
}

#[test]
fn favicon_discovery_never_widens_the_image_proxy_url_envelope() {
    let icon = |raw: &str| {
        favicon_url(
            &json!({"page_url":"https://example.com/", "icon_url": raw}),
            "https://example.com/",
        )
    };
    assert_eq!(
        icon("https://example.com:443/i.png#part"),
        Some("https://example.com/i.png".into())
    );
    for raw in [
        "http://example.com/i",
        "file:///i",
        "data:image/png,a",
        "https://user:pass@example.com/i",
        "https://example.com:444/i",
    ] {
        assert_eq!(icon(raw), None);
    }
    assert_eq!(favicon_url(&Value::Null, "https://example.com/"), None);
    assert_eq!(
        favicon_url(
            &json!({"page_url":"https://example.com/new", "icon_url":"https://example.com/new.png"}),
            "https://example.com/old"
        ),
        None
    );
    assert_eq!(
        icon(&format!("https://example.com/{}", "x".repeat(4096))),
        None
    );
}

#[test]
fn windows_listeners_keep_local_ones_once_and_survive_bad_output() {
    let rows = r#"[
            {"address":"127.0.0.1","port":3000,"pid":10,"process":"node"},
            {"address":"::1","port":3000,"pid":10,"process":"node"},
            {"address":"192.168.1.4","port":22,"pid":11,"process":"ssh"},
            {"address":"0.0.0.0","port":99999,"pid":12,"process":"bad"},
            {"address":"::","port":5173,"pid":13,"process":null}
        ]"#;
    assert_eq!(
        parse_windows_listeners(rows),
        vec![(10, "node".to_owned(), 3000), (13, String::new(), 5173)]
    );
    let one = r#"{"address":"127.0.0.1","port":80,"pid":1,"process":"x"}"#;
    assert_eq!(parse_windows_listeners(one).len(), 1);
    assert!(parse_windows_listeners("Access denied").is_empty());
}

#[test]
fn a_process_folder_is_read_on_this_platform() {
    let here = crate::util::canonicalize(&std::env::current_dir().unwrap()).unwrap();
    assert_eq!(in_folder(std::process::id(), &here), Some(true));
    let other = here.join("not-the-checkout");
    assert_eq!(in_folder(std::process::id(), &other), Some(false));
    assert_eq!(in_folder(u32::MAX - 1, &here), None);
}

#[test]
fn a_crashing_page_gets_three_growing_tries_in_its_window() {
    let start = Instant::now();
    let mut crashes = Crashes::default();
    assert_eq!(crashes.next_delay(start), Some(Duration::from_secs(1)));
    assert_eq!(crashes.next_delay(start), Some(Duration::from_secs(2)));
    assert_eq!(crashes.next_delay(start), Some(Duration::from_secs(4)));
    assert_eq!(crashes.next_delay(start), None);
    let later = start + CRASH_WINDOW + Duration::from_secs(1);
    assert_eq!(crashes.next_delay(later), Some(Duration::from_secs(1)));
}

#[test]
fn file_addresses_escape_what_a_path_can_hold() {
    assert_eq!(
        file_url(Path::new("/a b/c#d.html")),
        "file:///a%20b/c%23d.html"
    );
    assert_eq!(
        file_url(Path::new(r"C:\work\e.html")).replace("%5C", "/"),
        "file:///C:/work/e.html"
    );
}

#[test]
fn base64_round_trips_jpeg_bytes() {
    assert_eq!(decode_base64("Zm9vYmFy"), b"foobar");
    assert_eq!(decode_base64("Zm8="), b"fo");
    assert_eq!(decode_base64("/9j/"), [0xff, 0xd8, 0xff]);
}

#[test]
fn listeners_are_read_from_each_tool() {
    let lsof = "p12\ncnode\nf20\nn*:3000\nf21\nn[::1]:3000\np30\ncvite\nf1\nn127.0.0.1:5173\n";
    assert_eq!(
        parse_lsof(lsof),
        [(12, "node".to_owned(), 3000), (30, "vite".to_owned(), 5173)]
    );
    let netstat = "  TCP    127.0.0.1:8080   0.0.0.0:0   LISTENING   4242\n\
                       TCP    10.0.0.1:50000   1.2.3.4:443   ESTABLISHED   7\n";
    assert_eq!(parse_netstat(netstat), [(4242, String::new(), 8080)]);
}

#[test]
fn a_snapshot_expression_carries_its_limit_inside_the_cap() {
    let expression = snapshot_expression(10_000);
    assert!(expression.contains("const LIMIT = 300;"), "{expression}");
    assert!(expression.contains("const TEXT_LIMIT = 4000;"));
    assert!(snapshot_expression(40).contains("const LIMIT = 40;"));
    assert!(snapshot_expression(0).contains("const LIMIT = 1;"));
}

#[test]
fn a_snapshot_limit_is_what_was_asked_and_never_more() {
    assert_eq!(snapshot_limit(None), DEFAULT_SNAPSHOT_ELEMENTS);
    assert_eq!(snapshot_limit(Some(40)), 40);
    assert_eq!(snapshot_limit(Some(0)), 1);
    assert_eq!(snapshot_limit(Some(9_999)), MAX_SNAPSHOT_ELEMENTS);
    assert_eq!(snapshot_limit(Some(u64::MAX)), MAX_SNAPSHOT_ELEMENTS);
}

#[test]
fn a_snapshot_is_cut_again_so_a_page_cannot_flood_an_agent() {
    let page = json!({
        "url": format!("http://a/{}", "u".repeat(MAX_ADDRESS_CHARS)),
        "title": "  t  ",
        "elements": [
            {
                "role": "button",
                "name": "n".repeat(MAX_NAME_CHARS + 80),
                "selector": "#field",
                "value": "v".repeat(MAX_VALUE_CHARS + 80),
            },
            { "role": "link", "name": "second", "selector": "#two" },
        ],
        "text": "t".repeat(MAX_SNAPSHOT_TEXT + 10),
        "truncated": false,
    });
    let snapshot = capped_snapshot(&page, 10);
    assert_eq!(snapshot["title"], "t");
    assert_eq!(
        snapshot["url"].as_str().unwrap().chars().count(),
        MAX_ADDRESS_CHARS
    );
    let first = &snapshot["elements"][0];
    assert_eq!(
        first["name"].as_str().unwrap().chars().count(),
        MAX_NAME_CHARS
    );
    assert_eq!(
        first["value"].as_str().unwrap().chars().count(),
        MAX_VALUE_CHARS
    );
    assert_eq!(first["selector"], "#field");
    assert_eq!(
        snapshot["text"].as_str().unwrap().chars().count(),
        MAX_SNAPSHOT_TEXT
    );
    assert_eq!(snapshot["truncated"], true, "the text was cut here");
    assert_eq!(snapshot["elements"].as_array().unwrap().len(), 2);
    assert_eq!(snapshot["elements"][1]["selector"], "#two");
}

#[test]
fn a_snapshot_stops_at_its_limit_and_says_so() {
    let elements: Vec<Value> = (0..5)
        .map(|index| json!({ "role": "button", "name": format!("b{index}"), "selector": format!("#b{index}") }))
        .collect();
    let page = json!({ "elements": elements, "text": "x" });
    let bounded = capped_snapshot(&page, 2);
    assert_eq!(bounded["elements"].as_array().unwrap().len(), 2);
    assert_eq!(bounded["truncated"], true);
    let whole = capped_snapshot(&page, 5);
    assert_eq!(whole["truncated"], false, "everything that came back fit");
    let empty = capped_snapshot(&json!({}), 10);
    assert_eq!(empty["elements"], json!([]));
    assert_eq!(empty["text"], "");
    assert_eq!(empty["truncated"], false);
}

#[test]
fn a_snapshot_omits_oversized_selectors_without_retargeting_them() {
    let page = json!({ "elements": [
        { "selector": format!("#{}", "x".repeat(MAX_SELECTOR_CHARS)), "name": "long" },
        { "selector": "#short", "name": "short" }
    ] });
    let snapshot = capped_snapshot(&page, 10);
    assert_eq!(snapshot["truncated"], true);
    assert_eq!(snapshot["elements"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["elements"][0]["selector"], "#short");
}

#[test]
fn a_snapshot_preserves_trailing_selector_escape_spaces() {
    let page = json!({ "elements": [{ "selector": "#ends\\ " }] });
    let snapshot = capped_snapshot(&page, 10);
    assert_eq!(snapshot["elements"][0]["selector"], "#ends\\ ");
}

#[test]
fn an_expression_answers_whole_or_only_with_its_size() {
    let small = bounded_result(json!({ "a": 1 }));
    assert_eq!(small, json!({ "value": { "a": 1 }, "truncated": false }));
    let fits = "x".repeat(MAX_RESULT_BYTES - 2);
    assert_eq!(
        bounded_result(json!(fits))["truncated"],
        false,
        "a value of exactly the cap still fits"
    );
    let cut = bounded_result(json!("x".repeat(MAX_RESULT_BYTES)));
    assert_eq!(cut["truncated"], true);
    assert_eq!(
        cut["bytes"].as_u64().unwrap(),
        u64::try_from(MAX_RESULT_BYTES + 2).unwrap()
    );
    assert!(cut.get("value").is_none(), "a cut value is never sent");
}

#[test]
fn an_expression_is_refused_past_its_cap() {
    assert_eq!(checked_expression("1 + 1").unwrap(), "1 + 1");
    let at_cap = "x".repeat(MAX_EXPRESSION_CHARS);
    assert!(checked_expression(&at_cap).is_ok());
    let long = "x".repeat(MAX_EXPRESSION_CHARS + 1);
    assert!(checked_expression(&long).is_err());
}

#[test]
fn a_wait_is_bounded_in_time_and_in_rounds() {
    assert_eq!(
        wait_timeout(None),
        Duration::from_millis(DEFAULT_WAIT_MS),
        "five seconds unless asked"
    );
    assert_eq!(
        wait_timeout(Some(600_000)),
        Duration::from_millis(MAX_WAIT_MS)
    );
    assert_eq!(wait_timeout(Some(0)), Duration::from_millis(1));
    assert!(
        WAIT_POLL * WAIT_ROUNDS >= Duration::from_millis(MAX_WAIT_MS),
        "the rounds outlast the longest wait"
    );
}

#[test]
fn the_page_expressions_quote_what_they_carry() {
    let awkward = r#"a"b'c\d`e"#;
    let quoted = Value::String(awkward.to_owned()).to_string();
    for expression in [
        focus_expression(awkward),
        scroll_expression(Some(awkward)),
        wait_selector_expression(awkward),
        wait_text_expression(awkward),
        parses_expression(awkward),
        scroll_expression(None),
        snapshot_expression(5),
    ] {
        assert!(!expression.contains("__"), "{expression}");
    }
    assert!(focus_expression(awkward).contains(&quoted));
    assert!(wait_selector_expression(awkward).contains(&quoted));
    assert!(wait_text_expression(awkward).contains(&quoted));
    assert!(parses_expression(awkward).contains(&quoted));
    assert!(scroll_expression(Some(awkward)).contains(&quoted));
    assert!(
        scroll_expression(None).contains("const selector = null;"),
        "the page is the target when no selector is named"
    );
}
