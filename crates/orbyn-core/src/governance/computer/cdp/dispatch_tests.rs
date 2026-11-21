//! Real WebSocket controls for deadlines, late ids, cancellation and lifecycle delivery.

use super::*;

#[tokio::test]
async fn idle_document_events_are_delivered_without_a_command_reading_the_socket() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        released.await.unwrap();
        for event in [
            json!({"sessionId":"session","method":"Page.fileChooserOpened","params":{"backendNodeId":7}}),
            json!({"sessionId":"session","method":"Page.frameNavigated","params":{"frame":{"id":"main"}}}),
        ] {
            socket.send(Message::text(event.to_string())).await.unwrap();
        }
        while socket.next().await.is_some_and(|frame| frame.is_ok()) {}
    });
    let (socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
    let connection = Connection::new(socket);
    let changed = std::sync::Arc::new(tokio::sync::Notify::new());
    {
        let notify = changed.clone();
        let mut state = connection.client.state();
        state.session_id = "session".into();
        state.on_document_change = Some(std::sync::Arc::new(move || notify.notify_one()));
    }
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_millis(500), changed.notified())
        .await
        .unwrap();
    {
        let state = connection.client.state();
        assert_eq!(state.document.generation, 1);
        assert_eq!(state.kept[0]["method"], "Page.fileChooserOpened");
    }
    drop(connection);
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn frame_deadline_discards_a_real_late_reply_and_preserves_events() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (started, mut seen) = tokio::sync::mpsc::channel(2);
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let mut old = Vec::new();
        let mut late_sent = false;
        while let Some(Ok(frame)) = socket.next().await {
            let Ok(text) = frame.to_text() else { continue };
            let request: Value = serde_json::from_str(text).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => json!({"targetInfos":[{"type":"page","targetId":"target"}]}),
                "Target.attachToTarget" => json!({"sessionId":"session"}),
                "Page.getFrameTree" => json!({"frameTree":{"frame":{"id":"main"}}}),
                "Page.captureScreenshot" if old.len() < 2 => {
                    old.push(request["id"].clone());
                    started.send(()).await.unwrap();
                    continue;
                }
                "Page.captureScreenshot" => {
                    for id in &old {
                        socket
                            .send(Message::text(
                                json!({"id":id,"result":{"data":"stale"}}).to_string(),
                            ))
                            .await
                            .unwrap();
                    }
                    for message in [
                        json!({"id":999999,"result":{"data":"unknown"}}),
                        json!({"sessionId":"session","method":"Page.frameNavigated","params":{"frame":{"id":"main"}}}),
                        json!({"sessionId":"session","method":"Page.fileChooserOpened","params":{"backendNodeId":7}}),
                    ] {
                        socket
                            .send(Message::text(message.to_string()))
                            .await
                            .unwrap();
                    }
                    late_sent = true;
                    json!({"data":"fresh"})
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
        late_sent
    });
    let mut session = BrowserSession::connect(&format!("ws://{address}"), None)
        .await
        .unwrap();
    let before = session.document_generation();
    for _ in 0..2 {
        let capture = tokio::spawn(async move {
            let result = session.frame(60).await;
            (session, result)
        });
        seen.recv().await.unwrap();
        // Advance after the browser receives the capture, then allow a small
        // scheduling margin. A CALL_TIMEOUT capture fails this deadline check.
        tokio::time::pause();
        tokio::time::advance(FRAME_TIMEOUT).await;
        let outcome = tokio::time::timeout(Duration::from_millis(100), capture).await;
        tokio::time::resume();
        let (returned, result) = outcome.expect("frame did not use FRAME_TIMEOUT").unwrap();
        session = returned;
        let error = result.unwrap_err();
        assert!(error.to_string().contains("did not answer"));
    }
    assert_eq!(session.connection.client.frame_timeouts(), 2);
    assert_eq!(session.frame(60).await.unwrap(), "fresh");
    assert!(session.document_generation() > before);
    assert_eq!(session.take_events()[0]["method"], "Page.fileChooserOpened");
    drop(session);
    assert!(server.await.unwrap(), "fixture never sent the late reply");
}

#[tokio::test]
async fn dropping_the_session_wakes_its_frame_and_releases_the_reader() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (started, seen) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        let mut started = Some(started);
        while let Some(Ok(frame)) = socket.next().await {
            let Ok(text) = frame.to_text() else { continue };
            let request: Value = serde_json::from_str(text).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "Target.getTargets" => json!({"targetInfos":[{"type":"page","targetId":"target"}]}),
                "Target.attachToTarget" => json!({"sessionId":"session"}),
                "Page.getFrameTree" => json!({"frameTree":{"frame":{"id":"main"}}}),
                "Page.captureScreenshot" => {
                    started.take().unwrap().send(()).unwrap();
                    continue;
                }
                _ => json!({}),
            };
            socket
                .send(Message::text(
                    json!({"id":request["id"],"result":result}).to_string(),
                ))
                .await
                .unwrap();
        }
    });
    let session = BrowserSession::connect(&format!("ws://{address}"), None)
        .await
        .unwrap();
    let client = std::sync::Arc::downgrade(&session.connection.client);
    let capture = tokio::spawn(session.pending_frame(60));
    seen.await.unwrap();
    drop(session);
    let error = tokio::time::timeout(Duration::from_millis(500), capture)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("closed"));
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();
    assert!(
        client.upgrade().is_none(),
        "reader retained the connection after session drop"
    );
}
