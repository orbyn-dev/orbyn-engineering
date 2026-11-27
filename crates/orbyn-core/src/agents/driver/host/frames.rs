//! The wire format of docs/engine-host-protocol.md: one JSON object per line. Parsing is
//! strict about the fields the core acts on and ignores the rest, and it never guesses
//! what a frame meant: a line that does not fit is an error the caller counts and drops.

use serde_json::{Map, Value, json};

/// The protocol integer this app speaks. A host that says another number is a hard mismatch.
pub const PROTOCOL: u64 = 1;

/// A frame over this many bytes is a protocol error (the line, without its newline).
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// Requests the host lists in `hello.capabilities` before the core may send them. The
/// rest (`open`, `send`, `respond`, `cancel`, `closeSession`, `ping`, `close`, `init`) are
/// always available.
const GATED: &[&str] = &[
    "steer",
    "fork",
    "stopTask",
    "background",
    "rollback",
    "readHistory",
    "readSubagent",
    "usage",
    "models",
    "probe",
];

/// Whether `request` needs a capability the host must have announced.
pub fn is_gated(request: &str) -> bool {
    GATED.contains(&request)
}

/// Why a line could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    NotJson,
    NotAnObject,
    /// No `type`, or one this protocol version does not have.
    UnknownType(String),
    /// A field the core needs is missing or has the wrong type.
    Invalid(&'static str),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotJson => f.write_str("a line that is not JSON"),
            Self::NotAnObject => f.write_str("a frame that is not an object"),
            Self::UnknownType(kind) => write!(f, "a frame of unknown type {kind:?}"),
            Self::Invalid(field) => write!(f, "a frame with a bad {field}"),
        }
    }
}

/// The closed set of error kinds the core can act on without reading a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Auth,
    RateLimit,
    Network,
    InvalidRequest,
    Unsupported,
    Cancelled,
    Engine,
    Host,
    SessionLost,
    /// Absent or not in the set.
    Unknown,
}

impl ErrorKind {
    fn parse(text: Option<&str>) -> Self {
        match text {
            Some("auth") => Self::Auth,
            Some("rate_limit") => Self::RateLimit,
            Some("network") => Self::Network,
            Some("invalid_request") => Self::InvalidRequest,
            Some("unsupported") => Self::Unsupported,
            Some("cancelled") => Self::Cancelled,
            Some("engine") => Self::Engine,
            Some("host") => Self::Host,
            Some("session_lost") => Self::SessionLost,
            _ => Self::Unknown,
        }
    }
}

/// An error as it crosses the wire: a name, a message, an optional kind and a cause chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    pub name: String,
    pub message: String,
    pub kind: ErrorKind,
    pub cause: Option<Box<HostError>>,
}

impl HostError {
    /// Reads `{name, message, kind, cause}`; also accepts the request-level wrapper
    /// `{method, cause}`, which has no message of its own.
    pub fn parse(value: &Value) -> Self {
        let text = |key: &str| value.get(key).and_then(Value::as_str);
        let cause = value
            .get("cause")
            .filter(|cause| cause.is_object())
            .map(|cause| Box::new(Self::parse(cause)));
        Self {
            name: text("name").unwrap_or("Error").to_owned(),
            message: text("message").unwrap_or_default().to_owned(),
            kind: ErrorKind::parse(text("kind")),
            cause,
        }
    }

    /// The host's fixed sentence when this build carries no SDK for the family. It is the one
    /// answer that names a packaging gap, and the core records it rather than guessing.
    pub fn sdk_missing(&self) -> bool {
        self.message_text()
            .is_some_and(|text| text.ends_with("SDK is not available in this build."))
    }

    /// The first kind found walking down the chain, so a wrapper without one still acts on
    /// the cause's.
    pub fn effective_kind(&self) -> ErrorKind {
        let mut at = Some(self);
        while let Some(error) = at {
            if error.kind != ErrorKind::Unknown {
                return error.kind;
            }
            at = error.cause.as_deref();
        }
        ErrorKind::Unknown
    }

    /// The first non-empty message walking down the chain.
    pub fn message_text(&self) -> Option<&str> {
        let mut at = Some(self);
        while let Some(error) = at {
            if !error.message.trim().is_empty() {
                return Some(error.message.trim());
            }
            at = error.cause.as_deref();
        }
        None
    }

    /// How many errors the chain holds, itself included.
    pub fn depth(&self) -> usize {
        1 + self.cause.as_ref().map_or(0, |cause| cause.depth())
    }

    /// Whether a name anywhere in the chain is `name`.
    pub fn has_name(&self, name: &str) -> bool {
        self.name == name
            || self
                .cause
                .as_ref()
                .is_some_and(|cause| cause.has_name(name))
    }
}

/// The answer to one request.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub id: u64,
    pub result: Result<Value, HostError>,
}

/// The SDK a host runs: the package name and version its hello names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostSdk {
    pub name: String,
    pub version: String,
}

/// What `hello` says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    pub protocol: u64,
    pub host: String,
    pub family: String,
    /// Both halves of the SDK are needed; a hello with neither, or one, carries none.
    pub sdk: Option<HostSdk>,
    pub capabilities: Vec<String>,
}

/// `permission` and `question` share a shape.
#[derive(Clone, Debug, PartialEq)]
pub struct AskFrame {
    pub send_id: u64,
    pub request_id: String,
    pub tool: String,
    pub input: Value,
    /// The engine's own choices (ACP option kinds), exactly as received; `None` when it sent none.
    pub options: Option<Value>,
}

/// Why a run ended, by `runDone.result.reason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Completed,
    Cancelled,
    Failed,
    MaxTurns,
    Refused,
    /// A reason this protocol version does not have.
    Other,
}

/// A `retry` hint exactly as the host sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryHint {
    pub after_ms: Option<u64>,
    pub why: Option<String>,
    pub resets_at: Option<i64>,
}

/// The end of one `send`.
#[derive(Clone, Debug, PartialEq)]
pub struct RunDone {
    pub send_id: u64,
    pub ok: bool,
    pub reason: Option<Reason>,
    pub error: Option<HostError>,
    /// `reusable`, `broken`, or anything else (read as unknown).
    pub disposition: Option<String>,
    pub retry: Option<RetryHint>,
    /// The run's result usage, when the host reports one. Cursor sends it; other families
    /// may too, and only their own mapper decides what to do with it.
    pub usage: Option<Value>,
}

/// A frame the host sent.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    Hello(Hello),
    Response(Response),
    Delta {
        send_id: u64,
        update: Value,
    },
    Permission(AskFrame),
    Question(AskFrame),
    Session {
        send_id: u64,
        engine_session_id: Option<String>,
        turn_id: Option<String>,
    },
    RunDone(RunDone),
    Log {
        level: String,
        /// The event the host attached, when it sent one: `envAdded`, `holdingResult`, `malformedFrame`, ...
        event: Option<Value>,
    },
    Background {
        session_id: String,
        event: Value,
    },
}

fn uint(object: &Map<String, Value>, key: &'static str) -> Result<u64, FrameError> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(FrameError::Invalid(key))
}

fn string(object: &Map<String, Value>, key: &'static str) -> Result<String, FrameError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(FrameError::Invalid(key))
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn reason(text: Option<&str>) -> Option<Reason> {
    text.map(|text| match text {
        "completed" => Reason::Completed,
        "cancelled" => Reason::Cancelled,
        "failed" => Reason::Failed,
        "maxTurns" => Reason::MaxTurns,
        "refused" => Reason::Refused,
        _ => Reason::Other,
    })
}

fn ask(object: &Map<String, Value>) -> Result<AskFrame, FrameError> {
    Ok(AskFrame {
        send_id: uint(object, "sendId")?,
        request_id: string(object, "requestId")?,
        tool: object
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        input: object.get("input").cloned().unwrap_or(Value::Null),
        options: object
            .get("options")
            .filter(|value| !value.is_null())
            .cloned(),
    })
}

/// `disposition` and `retry` sit in `result` today and may sit at the top level of
/// `runDone`; both places are read, `result` first.
fn field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object
        .get("result")
        .and_then(|result| result.get(key))
        .or_else(|| object.get(key))
        .filter(|value| !value.is_null())
}

fn run_done(object: &Map<String, Value>) -> Result<RunDone, FrameError> {
    let ok = object
        .get("ok")
        .and_then(Value::as_bool)
        .ok_or(FrameError::Invalid("ok"))?;
    let retry = field(object, "retry")
        .and_then(Value::as_object)
        .map(|retry| RetryHint {
            after_ms: retry.get("afterMs").and_then(Value::as_u64),
            why: optional_string(retry, "why"),
            resets_at: retry.get("resetsAt").and_then(Value::as_i64),
        });
    Ok(RunDone {
        send_id: uint(object, "sendId")?,
        ok,
        reason: reason(
            object
                .get("result")
                .and_then(|result| result.get("reason"))
                .and_then(Value::as_str),
        ),
        error: object
            .get("error")
            .filter(|error| error.is_object())
            .map(HostError::parse),
        disposition: field(object, "threadDisposition")
            .and_then(Value::as_str)
            .map(str::to_owned),
        retry,
        usage: field(object, "usage").cloned(),
    })
}

/// Reads one line. The caller has already enforced `MAX_FRAME_BYTES`.
pub fn decode(line: &str) -> Result<Incoming, FrameError> {
    let value: Value = serde_json::from_str(line).map_err(|_| FrameError::NotJson)?;
    let Value::Object(object) = value else {
        return Err(FrameError::NotAnObject);
    };
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| FrameError::UnknownType(String::new()))?;
    Ok(match kind {
        "hello" => Incoming::Hello(Hello {
            protocol: uint(&object, "protocol")?,
            host: optional_string(&object, "host").unwrap_or_default(),
            family: string(&object, "family")?,
            sdk: object.get("sdk").and_then(|sdk| {
                let sdk = sdk.as_object()?;
                Some(HostSdk {
                    name: sdk.get("name")?.as_str()?.to_owned(),
                    version: sdk.get("version")?.as_str()?.to_owned(),
                })
            }),
            capabilities: object
                .get("capabilities")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        }),
        "response" => {
            let id = uint(&object, "id")?;
            let ok = object
                .get("ok")
                .and_then(Value::as_bool)
                .ok_or(FrameError::Invalid("ok"))?;
            let result = if ok {
                Ok(object.get("result").cloned().unwrap_or(Value::Null))
            } else {
                Err(object
                    .get("error")
                    .map_or_else(|| HostError::parse(&json!({})), HostError::parse))
            };
            Incoming::Response(Response { id, result })
        }
        "delta" => Incoming::Delta {
            send_id: uint(&object, "sendId")?,
            update: object.get("update").cloned().unwrap_or(Value::Null),
        },
        "permission" => Incoming::Permission(ask(&object)?),
        "question" => Incoming::Question(ask(&object)?),
        "session" => Incoming::Session {
            send_id: uint(&object, "sendId")?,
            engine_session_id: optional_string(&object, "engineSessionId"),
            turn_id: optional_string(&object, "turnId"),
        },
        "runDone" => Incoming::RunDone(run_done(&object)?),
        "log" => Incoming::Log {
            level: optional_string(&object, "level").unwrap_or_else(|| "info".into()),
            event: object
                .get("event")
                .filter(|event| !event.is_null())
                .cloned(),
        },
        "background" => Incoming::Background {
            session_id: string(&object, "sessionId")?,
            event: object.get("event").cloned().unwrap_or(Value::Null),
        },
        other => return Err(FrameError::UnknownType(other.to_owned())),
    })
}

/// A request line, without its newline. `fields` must be an object (or null for none).
pub fn request(id: u64, kind: &str, fields: Value) -> String {
    let mut object = match fields {
        Value::Object(object) => object,
        _ => Map::new(),
    };
    object.insert("id".into(), json!(id));
    object.insert("type".into(), json!(kind));
    Value::Object(object).to_string()
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
