//! What an engine says about a failed run beyond its message: whether to try again, and
//! whether the conversation can go on. Today only the engine host reports either.

/// Whether the engine's conversation can take another turn after a failed one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThreadDisposition {
    /// The next turn can resume it.
    Reusable,
    /// The engine says it cannot.
    Broken,
    /// Nothing said, or the engine went away mid-turn.
    #[default]
    Unknown,
}

/// Why trying again may work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryWhy {
    RateLimit,
    Network,
    Overloaded,
}

/// A hint from the engine that the same turn may succeed later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryInfo {
    pub after_ms: Option<u64>,
    pub why: RetryWhy,
}

/// A failed run with everything the engine added. `EngineOutcome::Failed` stays for the
/// drivers that have only a message; both end a run the same way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub message: String,
    pub retry: Option<RetryInfo>,
    pub disposition: ThreadDisposition,
}

impl Failure {
    /// A failure with a message alone.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retry: None,
            disposition: ThreadDisposition::Unknown,
        }
    }
}
