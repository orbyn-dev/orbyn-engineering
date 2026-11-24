//! Starting one host process and doing the handshake with it. Nothing here restarts anything:
//! a process that does not come up is an error for the pool to count.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::ChildStdout;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::frames::{Hello, Incoming, MAX_FRAME_BYTES, PROTOCOL, decode, request};
use super::state::{CrashInfo, CrashKind};
use crate::agents::driver::pipes::{EngineProcess, spawn_engine_with};
use crate::credentials::SpawnSecret;
use crate::host::process::{ManagedChild, ResolvedCommand};

/// What to start and with what environment. A secret is never part of this: it is fetched
/// for each spawn and goes straight into the child's environment.
#[derive(Clone, Debug)]
pub struct HostSpec {
    pub program: PathBuf,
    /// Arguments after the program (`<family> [flags]`).
    pub args: Vec<OsString>,
    /// Extra environment for the host (its own flags, never a credential).
    pub env: Vec<(String, String)>,
    /// `init.instance`: one host process per family, or per instance where needed.
    pub instance: String,
}

/// What one line of the host's stdout turned out to be.
#[derive(Debug, PartialEq, Eq)]
pub enum Read {
    Line(String),
    /// Not UTF-8.
    Malformed,
    /// Over the frame cap: a protocol error, and the stream cannot be trusted after it.
    TooLong,
    Eof,
    Failed(String),
}

/// A started host: the process and both pipes, which tasks own.
pub(crate) struct Transport {
    pub child: ManagedChild,
    pub stderr: crate::agents::driver::pipes::StderrTail,
    pub lines: mpsc::Receiver<Read>,
    pub outbox: mpsc::UnboundedSender<String>,
    /// Cancelling closes the host's stdin, which is how it is told to exit.
    pub close_stdin: CancellationToken,
}

/// Why a host did not come up.
#[derive(Debug, PartialEq, Eq)]
pub enum ConnectError {
    /// The host speaks another protocol. Final.
    Mismatch { app: u64, host: u64 },
    /// Anything else; it counts as a crash.
    Failed(CrashInfo),
}

/// What the handshake learned.
#[derive(Debug)]
pub struct Ready {
    pub hello: Hello,
    /// What `init`'s answer said about the family's SDK: `Some(true)` when the host could not load it, `Some(false)` when it
    /// could, `None` when the family has no SDK or the host did not say.
    pub sdk_missing: Option<bool>,
}

/// Reads lines of at most `MAX_FRAME_BYTES` (without the newline) from the host's stdout.
async fn read_lines(stdout: ChildStdout, out: mpsc::Sender<Read>) {
    let mut reader = BufReader::new(stdout);
    let limit = MAX_FRAME_BYTES as u64 + 1;
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        let read = (&mut reader)
            .take(limit + 1)
            .read_until(b'\n', &mut buffer)
            .await;
        let item = match read {
            Ok(0) => Read::Eof,
            Ok(_) => {
                let ended = buffer.last() == Some(&b'\n');
                if ended {
                    buffer.pop();
                    if buffer.last() == Some(&b'\r') {
                        buffer.pop();
                    }
                }
                if buffer.len() > MAX_FRAME_BYTES {
                    Read::TooLong
                } else if !ended {
                    // The host closed its output in the middle of a line.
                    Read::Eof
                } else {
                    match String::from_utf8(std::mem::take(&mut buffer)) {
                        Ok(line) => Read::Line(line),
                        Err(_) => Read::Malformed,
                    }
                }
            }
            Err(error) => Read::Failed(error.to_string()),
        };
        let last = matches!(item, Read::Eof | Read::TooLong | Read::Failed(_));
        if out.send(item).await.is_err() || last {
            return;
        }
    }
}

async fn write_lines(
    mut stdin: tokio::process::ChildStdin,
    mut lines: mpsc::UnboundedReceiver<String>,
    close: CancellationToken,
    failed: mpsc::Sender<Read>,
) {
    loop {
        tokio::select! {
            biased;
            () = close.cancelled() => return,
            line = lines.recv() => {
                let Some(mut line) = line else { return };
                line.push('\n');
                let wrote = async {
                    stdin.write_all(line.as_bytes()).await?;
                    stdin.flush().await
                };
                tokio::select! {
                    () = close.cancelled() => return,
                    wrote = wrote => if let Err(error) = wrote {
                        let _ = failed.send(Read::Failed(error.to_string())).await;
                        return;
                    },
                }
            }
        }
    }
}

impl Transport {
    /// Starts the pipe tasks on a process.
    pub(crate) fn start(process: EngineProcess) -> Self {
        let EngineProcess {
            child,
            stdin,
            stdout,
            stderr,
        } = process;
        let (read_tx, lines) = mpsc::channel(1024);
        let (outbox, outbox_rx) = mpsc::unbounded_channel();
        let close_stdin = CancellationToken::new();
        tokio::spawn(read_lines(stdout, read_tx.clone()));
        tokio::spawn(write_lines(stdin, outbox_rx, close_stdin.clone(), read_tx));
        Self {
            child,
            stderr,
            lines,
            outbox,
            close_stdin,
        }
    }

    /// Starts `spec`. `secret`, when the person opted in, goes only into the child's environment.
    pub fn spawn(spec: &HostSpec, secret: Option<&SpawnSecret>) -> Result<Self, ConnectError> {
        let command = ResolvedCommand {
            program: spec.program.clone(),
            prefix: Vec::new(),
            path: spec.program.clone(),
        };
        let mut env: Vec<(&str, &str)> = spec
            .env
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        if let Some(secret) = secret {
            env.push((secret.env, secret.expose()));
        }
        let process = spawn_engine_with(&command, &spec.args, &std::env::temp_dir(), &env)
            .map_err(|error| failed(CrashKind::StartFailed, error.public_message()))?;
        Ok(Self::start(process))
    }

    /// Why a host that failed to come up did: its exit and its last words on stderr.
    pub async fn explain(&mut self) -> CrashInfo {
        let status = tokio::time::timeout(Duration::from_millis(300), self.child.wait())
            .await
            .ok()
            .and_then(Result::ok);
        self.stderr.drained().await;
        let mut info = CrashInfo::new(
            CrashKind::StartFailed,
            "The engine host stopped while starting.",
        )
        .with_stderr(&self.stderr.text());
        info.exit = status.map(super::state::ExitRecord::of);
        if let Some(exit) = info.exit {
            info.message = format!(
                "The engine host stopped while starting ({}).",
                exit.describe()
            );
        }
        info
    }
}

fn failed(kind: CrashKind, message: impl Into<String>) -> ConnectError {
    ConnectError::Failed(CrashInfo::new(kind, message))
}

/// Waits for `hello`, answers it with `init` and waits for `ready`. A different protocol is
/// a mismatch; every other problem, including silence, is a failure to start.
pub async fn handshake(
    transport: &mut Transport,
    spec: &HostSpec,
    family: &str,
    hello_timeout: Duration,
    init_timeout: Duration,
) -> Result<Ready, ConnectError> {
    let hello = match tokio::time::timeout(hello_timeout, transport.lines.recv()).await {
        Err(_) => {
            return Err(failed(
                CrashKind::HelloTimeout,
                format!(
                    "The engine host did not say hello within {} seconds.",
                    hello_timeout.as_secs().max(1)
                ),
            ));
        }
        Ok(None | Some(Read::Eof)) => return Err(ConnectError::Failed(transport.explain().await)),
        Ok(Some(Read::Line(line))) => match decode(&line) {
            Ok(Incoming::Hello(hello)) => hello,
            _ => {
                return Err(failed(
                    CrashKind::StartFailed,
                    "The engine host did not start with a hello.",
                ));
            }
        },
        Ok(Some(_)) => {
            return Err(failed(
                CrashKind::StartFailed,
                "The engine host sent an unusable first line.",
            ));
        }
    };
    if hello.protocol != PROTOCOL {
        return Err(ConnectError::Mismatch {
            app: PROTOCOL,
            host: hello.protocol,
        });
    }
    if hello.family != family {
        return Err(failed(
            CrashKind::StartFailed,
            format!("The engine host runs {:?}, not {family:?}.", hello.family),
        ));
    }
    let init = request(
        1,
        "init",
        serde_json::json!({"protocol": PROTOCOL, "instance": spec.instance, "logLevel": "info"}),
    );
    transport
        .outbox
        .send(init)
        .map_err(|_| failed(CrashKind::StartFailed, "The engine host closed its input."))?;
    let answered = async {
        loop {
            match transport.lines.recv().await {
                Some(Read::Line(line)) => match decode(&line) {
                    Ok(Incoming::Response(response)) if response.id == 1 => {
                        return response.result.map_err(|error| {
                            if error.has_name("ProtocolMismatch") {
                                ConnectError::Mismatch {
                                    app: PROTOCOL,
                                    host: hello.protocol,
                                }
                            } else {
                                failed(
                                    CrashKind::StartFailed,
                                    error
                                        .message_text()
                                        .unwrap_or("The engine host refused to start.")
                                        .to_owned(),
                                )
                            }
                        });
                    }
                    Ok(Incoming::Log { .. }) => {}
                    _ => {
                        return Err(failed(
                            CrashKind::StartFailed,
                            "The engine host answered init with something else.",
                        ));
                    }
                },
                Some(Read::Malformed) => {}
                Some(Read::Eof) | None => {
                    return Err(failed(
                        CrashKind::ClosedOutput,
                        "The engine host closed while starting.",
                    ));
                }
                Some(Read::TooLong | Read::Failed(_)) => {
                    return Err(failed(
                        CrashKind::Malformed,
                        "The engine host sent an unusable line while starting.",
                    ));
                }
            }
        }
    };
    let result = tokio::time::timeout(init_timeout, answered)
        .await
        .map_err(|_| {
            failed(
                CrashKind::HelloTimeout,
                "The engine host did not become ready in time.",
            )
        })??;
    Ok(Ready {
        hello,
        sdk_missing: result
            .get("sdkMissing")
            .and_then(serde_json::Value::as_bool),
    })
}
