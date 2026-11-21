//! One reader owns inbound CDP traffic. Callers own bounded, cancellable reply slots.

use super::*;
use futures_util::stream::SplitSink;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::{Notify, oneshot};
use tokio_util::sync::CancellationToken;

const MAX_PENDING: usize = 128;
type Reply = std::result::Result<Value, String>;

struct Pending {
    reply: oneshot::Sender<Reply>,
    unless_dialog: bool,
    attach: bool,
}

#[derive(Default)]
pub(super) struct State {
    pub document: Document,
    pub session_id: String,
    pub on_document_change: Option<Arc<dyn Fn() + Send + Sync>>,
    pub kept: Vec<Value>,
    pub navigation_events: VecDeque<Value>,
    next_id: u64,
    pending: HashMap<u64, Pending>,
    failure: Option<String>,
    frame_timeouts: u64,
}

pub(super) struct Client {
    writer: tokio::sync::Mutex<SplitSink<Socket, Message>>,
    state: Mutex<State>,
    pub changed: Notify,
    stopped: CancellationToken,
}

impl Client {
    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn fail(&self, reason: String) {
        let pending = {
            let mut state = self.state();
            state.failure.get_or_insert_with(|| reason.clone());
            std::mem::take(&mut state.pending)
        };
        for (_, pending) in pending {
            let _ = pending.reply.send(Err(reason.clone()));
        }
        self.changed.notify_waiters();
    }

    pub fn is_closed(&self) -> bool {
        self.state().failure.is_some()
    }

    #[cfg(test)]
    pub fn frame_timeouts(&self) -> u64 {
        self.state().frame_timeouts
    }

    fn receive(&self, message: Value) {
        // A reply can only belong to the slot registered for this exact id.
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            let pending = {
                let mut state = self.state();
                let pending = state.pending.remove(&id);
                // The next event can immediately follow attachment's reply,
                // before connect() is scheduled to store its session id.
                if pending.as_ref().is_some_and(|pending| pending.attach)
                    && let Some(session) =
                        message.pointer("/result/sessionId").and_then(Value::as_str)
                {
                    state.session_id = session.to_owned();
                }
                pending
            };
            if let Some(pending) = pending {
                let reply = match message.get("error") {
                    Some(error) => Err(error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("command refused")
                        .to_owned()),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = pending.reply.send(reply);
            }
            return;
        }
        let (notify, interrupted) = {
            let mut state = self.state();
            let mut notify = None;
            let mut interrupted = Vec::new();
            let session = message.get("sessionId").and_then(Value::as_str);
            let owned = session == Some(state.session_id.as_str());
            if owned {
                if state.document.note(&message) {
                    notify = state.on_document_change.clone();
                }
                if state.is_navigation_signal(&message) {
                    if state.navigation_events.len() == MAX_KEPT {
                        state.navigation_events.pop_front();
                    }
                    state.navigation_events.push_back(message.clone());
                }
                if state.document.dialog {
                    let ids: Vec<_> = state
                        .pending
                        .iter()
                        .filter_map(|(id, pending)| pending.unless_dialog.then_some(*id))
                        .collect();
                    for id in ids {
                        if let Some(pending) = state.pending.remove(&id) {
                            interrupted.push(pending);
                        }
                    }
                }
            }
            // Downloads are browser-wide. Page bridge events belong only to the
            // attached session; another target cannot supply its file chooser.
            if owned
                || session.is_none()
                    && message["method"]
                        .as_str()
                        .is_some_and(|method| method.starts_with("Browser."))
            {
                keep_event(&mut state.kept, message);
            }
            (notify, interrupted)
        };
        // Callbacks may take tab locks, so they never run under the connection lock.
        // Complete them before delivering the next reply and its document fence.
        if let Some(notify) = notify {
            notify();
        }
        for pending in interrupted {
            let _ = pending.reply.send(Err("a dialog is open".into()));
        }
        self.changed.notify_waiters();
    }

    pub async fn send(
        &self,
        method: &str,
        params: Value,
        session: Option<String>,
        unless_dialog: bool,
        limit: Duration,
    ) -> Result<Value> {
        let (reply, received) = oneshot::channel();
        let id = {
            let mut state = self.state();
            if let Some(reason) = &state.failure {
                return Err(failed(reason));
            }
            if unless_dialog && state.document.dialog {
                return Err(failed("a dialog is open"));
            }
            if state.pending.len() >= MAX_PENDING {
                return Err(failed("too many browser commands are pending"));
            }
            state.next_id = state
                .next_id
                .checked_add(1)
                .ok_or_else(|| failed("browser command ids exhausted"))?;
            let id = state.next_id;
            state.pending.insert(
                id,
                Pending {
                    reply,
                    unless_dialog,
                    attach: method == "Target.attachToTarget",
                },
            );
            id
        };
        let _slot = Slot { client: self, id };
        let mut message = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            message["sessionId"] = json!(session);
        }
        let work = async {
            self.writer
                .lock()
                .await
                .send(Message::text(message.to_string()))
                .await
                .map_err(|error| {
                    self.fail("the browser connection could not send a command".into());
                    failed(error)
                })?;
            received
                .await
                .map_err(|_| failed("the browser closed the connection"))?
                .map_err(failed)
        };
        tokio::select! {
            biased;
            () = self.stopped.cancelled() => Err(failed("the browser session closed")),
            result = tokio::time::timeout(limit, work) => match result {
                Ok(result) => result,
                Err(_) => {
                    if method == "Page.captureScreenshot" && limit == FRAME_TIMEOUT {
                        let count = {
                            let mut state = self.state();
                            state.frame_timeouts = state.frame_timeouts.saturating_add(1);
                            state.frame_timeouts
                        };
                        // Power-of-two reporting stays useful without flooding logs.
                        if count.is_power_of_two() {
                            tracing::warn!(method, count, "preview frame capture timed out");
                        }
                    }
                    Err(failed("the browser did not answer"))
                }
            }
        }
    }
}

/// Removes a reply slot on timeout, caller cancellation, failure and success.
struct Slot<'a> {
    client: &'a Client,
    id: u64,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.client.state().pending.remove(&self.id);
    }
}

/// This owner lives only in BrowserSession. Frame futures cannot keep a browser alive.
pub(super) struct Connection {
    pub client: Arc<Client>,
    reader: tokio::task::JoinHandle<()>,
}

impl Connection {
    pub fn new(socket: Socket) -> Self {
        let (writer, mut reader) = socket.split();
        let client = Arc::new(Client {
            writer: tokio::sync::Mutex::new(writer),
            state: Mutex::new(State::default()),
            changed: Notify::new(),
            stopped: CancellationToken::new(),
        });
        let receiving = client.clone();
        let task = tokio::spawn(async move {
            let reason = loop {
                match reader.next().await {
                    Some(Ok(Message::Close(_))) | None => break "the browser closed the connection",
                    Some(Err(_)) => break "the browser connection could not read a reply",
                    Some(Ok(message)) => {
                        if let Ok(text) = message.to_text()
                            && let Ok(value) = serde_json::from_str(text)
                        {
                            receiving.receive(value);
                        }
                    }
                }
            };
            receiving.fail(reason.into());
        });
        Self {
            client,
            reader: task,
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.client.stopped.cancel();
        self.client.fail("the browser session closed".into());
        self.reader.abort();
    }
}

impl State {
    /// Only main-frame signals can end a navigation; iframe traffic cannot evict them.
    fn is_navigation_signal(&self, event: &Value) -> bool {
        let params = &event["params"];
        match event.get("method").and_then(Value::as_str) {
            Some("Page.frameNavigated") => params["frame"].get("parentId").is_none(),
            Some("Page.lifecycleEvent" | "Page.navigatedWithinDocument") => {
                self.document.frame.is_empty()
                    || params["frameId"].as_str() == Some(self.document.frame.as_str())
            }
            Some("Page.loadEventFired") => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn attachment_installs_the_session_before_the_immediately_following_event() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let request: Value =
                serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            for message in [
                json!({"id":request["id"],"result":{"sessionId":"attached"}}),
                json!({"sessionId":"other","method":"Page.frameNavigated","params":{"frame":{"id":"other"}}}),
                json!({"sessionId":"other","method":"Page.fileChooserOpened","params":{"backendNodeId":99}}),
                json!({"sessionId":"other","method":"Browser.downloadProgress","params":{"guid":"other"}}),
                json!({"sessionId":"attached","method":"Page.fileChooserOpened","params":{"backendNodeId":7}}),
                json!({"method":"Browser.downloadProgress","params":{"guid":"browser"}}),
                json!({"sessionId":"attached","method":"Page.frameNavigated","params":{"frame":{"id":"main"}}}),
            ] {
                socket
                    .feed(Message::text(message.to_string()))
                    .await
                    .unwrap();
            }
            socket.flush().await.unwrap();
            while socket.next().await.is_some_and(|frame| frame.is_ok()) {}
        });
        let (socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
        let connection = Connection::new(socket);
        let changed = Arc::new(Notify::new());
        let notified = changed.clone();
        connection.client.state().on_document_change =
            Some(Arc::new(move || notified.notify_one()));
        connection
            .client
            .send(
                "Target.attachToTarget",
                json!({}),
                None,
                false,
                CALL_TIMEOUT,
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_millis(500), changed.notified())
            .await
            .unwrap();
        {
            let state = connection.client.state();
            assert_eq!(state.session_id, "attached");
            assert_eq!(
                state.document.generation, 1,
                "another session changed the document"
            );
            assert_eq!(state.document.frame, "main");
            assert_eq!(state.kept.len(), 2);
            assert_eq!(state.kept[0]["params"]["backendNodeId"], 7);
            assert_eq!(state.kept[1]["params"]["guid"], "browser");
        }
        drop(connection);
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn pending_commands_are_bounded_and_cancellation_returns_a_slot() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (full, filled) = oneshot::channel();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let mut count = 0;
            let mut full = Some(full);
            while let Some(Ok(message)) = socket.next().await {
                let Ok(text) = message.to_text() else {
                    continue;
                };
                let request: Value = serde_json::from_str(text).unwrap();
                count += 1;
                if count == MAX_PENDING {
                    full.take().unwrap().send(()).unwrap();
                }
                if request["method"] == "control" {
                    socket
                        .send(Message::text(
                            json!({"id":request["id"],"result":{"accepted":true}}).to_string(),
                        ))
                        .await
                        .unwrap();
                }
            }
        });
        let (socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
        let connection = Connection::new(socket);
        let mut waiting = Vec::new();
        for _ in 0..MAX_PENDING {
            let client = connection.client.clone();
            waiting.push(tokio::spawn(async move {
                client
                    .send("defer", json!({}), None, false, CALL_TIMEOUT)
                    .await
            }));
        }
        filled.await.unwrap();
        let error = connection
            .client
            .send("overflow", json!({}), None, false, CALL_TIMEOUT)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("too many"));
        let cancelled = waiting.pop().unwrap();
        cancelled.abort();
        assert!(cancelled.await.unwrap_err().is_cancelled());
        let accepted = connection
            .client
            .send("control", json!({}), None, false, CALL_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(accepted["accepted"], true);
        drop(connection);
        for pending in waiting {
            assert!(
                pending
                    .await
                    .unwrap()
                    .unwrap_err()
                    .to_string()
                    .contains("closed")
            );
        }
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
    }
}
