//! A tiny local HTTP server for the tracker readers' tests: no real network, one scripted answer per request,
//! and a record of every request it received.

use serde_json::Value;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One request as the server saw it. Header names are lower case.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    pub line: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
    /// The request body, when it sent a `content-length` that named one.
    pub body: String,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The path of the request target, still percent-encoded.
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or_default()
    }

    /// One decoded query value.
    pub fn param(&self, name: &str) -> Option<String> {
        let url = reqwest::Url::parse(&format!("http://fake{}", self.target)).ok()?;
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    }
}

/// What the server answers.
pub(crate) struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    hang: bool,
    with_length: bool,
}

impl Reply {
    pub fn raw(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
            hang: false,
            with_length: true,
        }
    }

    pub fn json(value: &Value) -> Self {
        Self::raw(200, value.to_string().into_bytes())
    }

    /// An error status whose body is raw tracker text that must never be passed on.
    pub fn status(code: u16) -> Self {
        Self::raw(
            code,
            br#"{"message":"raw-tracker-text secret-detail"}"#.to_vec(),
        )
    }

    pub fn hang() -> Self {
        let mut reply = Self::raw(200, Vec::new());
        reply.hang = true;
        reply
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Sends no `content-length`: the body ends when the connection closes.
    pub fn unsized_body(mut self) -> Self {
        self.with_length = false;
        self
    }
}

pub(crate) struct Server {
    /// `http://127.0.0.1:<port>`, without a trailing slash.
    pub base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Server {
    pub async fn start(handler: impl Fn(&Seen) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
        let handler: Arc<dyn Fn(&Seen) -> Reply + Send + Sync> = Arc::new(handler);
        let log = seen.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let log = log.clone();
                let handler = handler.clone();
                tokio::spawn(async move {
                    let mut raw = Vec::new();
                    let mut chunk = [0u8; 2048];
                    let header_end = loop {
                        if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                            break at + 4;
                        }
                        let read = stream.read(&mut chunk).await.unwrap_or(0);
                        if read == 0 {
                            return;
                        }
                        raw.extend_from_slice(&chunk[..read]);
                    };
                    let head = String::from_utf8_lossy(&raw[..header_end]).into_owned();
                    let mut lines = head.split("\r\n");
                    let line = lines.next().unwrap_or_default().to_owned();
                    let target = line.split(' ').nth(1).unwrap_or_default().to_owned();
                    let headers: Vec<(String, String)> = lines
                        .take_while(|l| !l.is_empty())
                        .filter_map(|l| l.split_once(':'))
                        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
                        .collect();
                    let length = headers
                        .iter()
                        .find(|(k, _)| k == "content-length")
                        .and_then(|(_, v)| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    while raw.len() < header_end + length {
                        let read = stream.read(&mut chunk).await.unwrap_or(0);
                        if read == 0 {
                            break;
                        }
                        raw.extend_from_slice(&chunk[..read]);
                    }
                    let end = (header_end + length).min(raw.len());
                    let body = String::from_utf8_lossy(&raw[header_end..end]).into_owned();
                    let request = Seen {
                        line,
                        target,
                        headers,
                        body,
                    };
                    log.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(request.clone());
                    let reply = handler(&request);
                    if reply.hang {
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        return;
                    }
                    let mut head = format!("HTTP/1.1 {} X\r\nconnection: close\r\n", reply.status);
                    for (name, value) in &reply.headers {
                        head.push_str(&format!("{name}: {value}\r\n"));
                    }
                    if reply.with_length {
                        head.push_str(&format!("content-length: {}\r\n", reply.body.len()));
                    }
                    head.push_str("\r\n");
                    let _ = stream.write_all(head.as_bytes()).await;
                    let _ = stream.write_all(&reply.body).await;
                    let _ = stream.shutdown().await;
                });
            }
        });
        Self { base, seen }
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
