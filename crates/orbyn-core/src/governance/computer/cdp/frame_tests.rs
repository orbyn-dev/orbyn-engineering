//! A frame capture the browser drops must give the session back.

use super::*;

#[tokio::test]
async fn a_capture_the_browser_never_answers_gives_the_session_back() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let (mut dropped, mut seen) = (None, false);
        while let Some(Ok(frame)) = socket.next().await {
            let Ok(text) = frame.to_text() else { continue };
            let request: Value = serde_json::from_str(text).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => {
                    json!({ "targetInfos": [{ "type": "page", "targetId": "target" }] })
                }
                "Target.attachToTarget" => json!({ "sessionId": "session" }),
                "Page.getFrameTree" => json!({ "frameTree": { "frame": { "id": "main" } } }),
                "Runtime.evaluate" => json!({ "result": { "value": "load 2" } }),
                // A capture sent as the document swaps: the browser drops it.
                "Page.captureScreenshot" if !seen => {
                    seen = true;
                    dropped = Some(request["id"].clone());
                    continue;
                }
                "Page.captureScreenshot" => json!({ "data": "frame" }),
                _ => json!({}),
            };
            // The dropped capture's reply turns up late, ahead of the next answer.
            if let Some(id) = dropped.take_if(|_| request["method"] == "Runtime.evaluate") {
                let late = json!({ "id": id, "result": { "data": "stale" } });
                socket.send(Message::text(late.to_string())).await.unwrap();
            }
            socket
                .send(Message::text(
                    json!({ "id": request["id"], "result": result }).to_string(),
                ))
                .await
                .unwrap();
        }
    });
    let mut session = BrowserSession::connect(&format!("ws://{address}"), None)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let error = session.frame(60).await.unwrap_err();
    let waited = started.elapsed();
    assert!(error.to_string().contains("did not answer"), "{error}");
    assert!(waited >= FRAME_TIMEOUT, "gave up after {waited:?}");
    assert!(waited < CALL_TIMEOUT / 2, "waited {waited:?}");
    // The late reply is dropped, and the session reads the new document and captures again.
    assert_eq!(session.title().await.unwrap(), "load 2");
    assert_eq!(session.frame(60).await.unwrap(), "frame");
    drop(session);
    server.await.unwrap();
}
