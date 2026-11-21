//! The real preview pump must leave the session available during a dropped frame.

use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn metadata_that_stops_answering_does_not_suspend_a_frame_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (health, checked) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let mut health = Some(health);
        while let Some(Ok(frame)) = socket.next().await {
            let Ok(text) = frame.to_text() else { continue };
            let request: Value = serde_json::from_str(text).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => json!({"targetInfos":[{"type":"page","targetId":"target"}]}),
                "Target.attachToTarget" => json!({"sessionId":"session"}),
                "Page.getFrameTree" => json!({"frameTree":{"frame":{"id":"main"}}}),
                "Page.captureScreenshot" | "Page.getNavigationHistory" => continue,
                "Runtime.evaluate" => {
                    if let Some(health) = health.take() {
                        health.send(()).unwrap();
                    }
                    json!({"result":{"value":true}})
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
    let tab = fixture_tab(session, "human");
    let _viewer = tab.frames.subscribe();
    let pumping = tokio::spawn(pump(tab.clone()));
    let deadline = tokio::time::timeout(Duration::from_secs(4), checked).await;
    tab.stop.cancel();
    tokio::time::timeout(Duration::from_secs(1), pumping)
        .await
        .unwrap()
        .unwrap();
    drop(tab);
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
    deadline
        .expect("metadata suspended the frame timeout and health check")
        .unwrap();
}

#[tokio::test]
async fn a_pending_pump_frame_does_not_hold_the_session() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (capturing, captured) = tokio::sync::oneshot::channel();
    let (refreshed, metadata) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let mut capturing = Some(capturing);
        let mut refreshed = Some(refreshed);
        while let Some(Ok(frame)) = socket.next().await {
            let Ok(text) = frame.to_text() else { continue };
            let request: Value = serde_json::from_str(text).unwrap();
            if capturing.is_none()
                && request["params"]["expression"] == super::super::favicon::FAVICON
                && let Some(refreshed) = refreshed.take()
            {
                refreshed.send(()).unwrap();
            }
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => json!({"targetInfos":[{"type":"page","targetId":"target"}]}),
                "Target.attachToTarget" => json!({"sessionId":"session"}),
                "Page.getFrameTree" => json!({"frameTree":{"frame":{"id":"main"}}}),
                "Page.captureScreenshot" => {
                    if let Some(capturing) = capturing.take() {
                        let _ = capturing.send(());
                    }
                    continue;
                }
                "Runtime.evaluate" => json!({"result":{"value":"responsive"}}),
                "Page.getNavigationHistory" => {
                    json!({"currentIndex":0,"entries":[{"id":1,"url":"http://fixture/"}]})
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
    let tab = fixture_tab(session, "human");
    let _viewer = tab.frames.subscribe();
    let manager = PreviewManager::default();
    lock(&manager.tabs).insert("tab".into(), tab.clone());
    let pumping = tokio::spawn(pump(tab.clone()));
    captured.await.unwrap();
    let reads = tokio::time::timeout(Duration::from_millis(500), async {
        let mut session = tab.session.lock().await;
        let title = session.title().await?;
        let history = session.history().await?;
        let icon = session.read("'favicon fixture'").await?;
        Ok::<_, crate::error::CoreError>((title, history.url, icon))
    })
    .await;
    let refresh = tokio::time::timeout(Duration::from_millis(1500), metadata).await;
    manager.close("thread", "tab").unwrap();
    let closed = tokio::time::timeout(Duration::from_secs(1), pumping).await;
    closed
        .expect("close waited behind the pending frame")
        .unwrap();
    drop(tab);
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        reads
            .expect("frame held the preview session mutex")
            .unwrap(),
        (
            "responsive".into(),
            "http://fixture/".into(),
            json!("responsive")
        )
    );
    refresh
        .expect("periodic metadata refresh waited behind the pending frame")
        .unwrap();
}
