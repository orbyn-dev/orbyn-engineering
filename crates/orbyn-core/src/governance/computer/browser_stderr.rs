//! Bounded startup diagnostics; keep draining the browser's pipe after startup.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::task::JoinHandle;

const MAX_BYTES: usize = 8192;
const DRAIN: Duration = Duration::from_millis(250);

#[derive(Default)]
struct Capture {
    bytes: Vec<u8>,
    capturing: bool,
}

pub(super) struct BrowserStderr {
    capture: Arc<Mutex<Capture>>,
    reader: JoinHandle<()>,
}

impl BrowserStderr {
    pub(super) fn start(mut pipe: impl AsyncRead + Unpin + Send + 'static) -> Self {
        let capture = Arc::new(Mutex::new(Capture {
            capturing: true,
            ..Capture::default()
        }));
        let buffer = capture.clone();
        let reader = tokio::spawn(async move {
            let mut chunk = [0; 4096];
            loop {
                let read = match pipe.read(&mut chunk).await {
                    Ok(0) => break,
                    Ok(read) => read,
                    // Diagnostics are best effort; the launch error is still the primary failure.
                    Err(_) => break,
                };
                let mut capture = buffer.lock().unwrap_or_else(|error| error.into_inner());
                if capture.capturing {
                    capture.bytes.extend_from_slice(&chunk[..read]);
                    let excess = capture.bytes.len().saturating_sub(MAX_BYTES);
                    capture.bytes.drain(..excess);
                }
            }
        });
        Self { capture, reader }
    }

    /// A descendant may retain the pipe after the browser exits: never wait for its EOF indefinitely.
    pub(super) async fn failure_text(&mut self) -> String {
        let _ = tokio::time::timeout(DRAIN, &mut self.reader).await;
        let capture = self
            .capture
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        crate::agents::driver::diagnostics::clean(&String::from_utf8_lossy(&capture.bytes))
    }

    pub(super) fn startup_complete(&self) {
        let mut capture = self
            .capture
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        capture.capturing = false;
        capture.bytes.clear();
    }
}

impl Drop for BrowserStderr {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn retains_only_a_bounded_tail_and_cleans_failure_text() {
        let (mut writer, pipe) = tokio::io::duplex(1024);
        let mut stderr = BrowserStderr::start(pipe);
        let send = tokio::spawn(async move {
            writer.write_all(&vec![b'x'; MAX_BYTES * 4]).await.unwrap();
            writer
                .write_all(
                    b"\nAuthorization: Bearer fixture-secret\n\x1b[31mstartup marker\x1b[0m\n",
                )
                .await
                .unwrap();
        });
        send.await.unwrap();
        let text = stderr.failure_text().await;
        assert!(text.len() <= MAX_BYTES + 128);
        assert!(text.contains("startup marker"));
        assert!(!text.contains("fixture-secret"));
        assert!(!text.contains('\x1b'));
    }

    #[tokio::test(start_paused = true)]
    async fn a_pipe_held_open_cannot_stall_failure_reporting() {
        let (_writer, pipe) = tokio::io::duplex(1024);
        let mut stderr = BrowserStderr::start(pipe);
        let before = tokio::time::Instant::now();
        assert!(stderr.failure_text().await.is_empty());
        assert_eq!(before.elapsed(), DRAIN);
    }

    #[tokio::test]
    async fn successful_startup_clears_the_tail_but_keeps_the_pipe_drained() {
        let (mut writer, pipe) = tokio::io::duplex(1024);
        let mut stderr = BrowserStderr::start(pipe);
        writer.write_all(b"startup note\n").await.unwrap();
        stderr.startup_complete();
        // More than the pipe's capacity: dropping the reader or ceasing reads would block this.
        writer.write_all(&vec![b'x'; MAX_BYTES * 4]).await.unwrap();
        drop(writer);
        assert!(stderr.failure_text().await.is_empty());
    }
}
