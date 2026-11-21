//! A sandbox for model-written scripts: JavaScript or TypeScript run in an engine with no way out
//! except one function that calls tools. It knows nothing of Orbyn's gate, audit or Crew; the
//! [`ToolHost`] it is given is the whole interface (see `docs/code-mode.md`).

mod quickjs;
mod strip;

pub use quickjs::QuickJsExecutor;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Every limit of a run, in one value so a test can set them small.
#[derive(Clone, Debug)]
pub struct Limits {
    pub heap_bytes: usize,
    pub stack_bytes: usize,
    /// Time the script itself computes. Time spent waiting on a tool call does not count.
    pub cpu: Duration,
    pub calls: usize,
    pub in_flight: usize,
    pub source_bytes: usize,
    pub log_lines: usize,
    pub log_bytes: usize,
    pub wall: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            heap_bytes: 64 * 1024 * 1024,
            stack_bytes: 512 * 1024,
            cpu: Duration::from_secs(10),
            calls: 200,
            in_flight: 8,
            source_bytes: 64 * 1024,
            log_lines: 500,
            log_bytes: 64 * 1024,
            wall: Duration::from_secs(15 * 60),
        }
    }
}

/// What makes `Date.now()` and `Math.random()` repeatable, so a run can be replayed.
#[derive(Clone, Copy, Debug)]
pub struct RunSeed {
    pub start_ms: i64,
    pub seed: u32,
}

/// Why a tool call failed, as the script sees it: `error.code` and `error.message`.
#[derive(Clone, Debug)]
pub struct ToolFailure {
    pub code: String,
    pub message: String,
}

/// The only door out of the sandbox.
#[async_trait]
pub trait ToolHost: Send + Sync {
    async fn call(&self, name: &str, args: Value) -> Result<Value, ToolFailure>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Syntax,
    Runtime,
    Limit,
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("{message}")]
pub struct RunError {
    pub kind: ErrorKind,
    pub message: String,
    pub line: Option<u32>,
    /// The `code` of an uncaught tool failure (`denied`, `invalid`, ...), when that ended the run.
    pub code: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Step {
    Done {
        result: Value,
        logs: Vec<String>,
        truncated: bool,
    },
    Failed {
        error: RunError,
        logs: Vec<String>,
        truncated: bool,
    },
    /// `until` passed; the run is alive and `drive` can be called again.
    Yielded,
}

#[async_trait]
pub trait CodeExecutor: Send + Sync {
    /// Parses `source` (TypeScript types are stripped) and prepares a run. Runs nothing.
    async fn prepare(
        &self,
        source: &str,
        limits: Limits,
        seed: RunSeed,
        host: Arc<dyn ToolHost>,
    ) -> Result<Box<dyn Run>, RunError>;
}

#[async_trait]
pub trait Run: Send {
    /// Drives the script until it finishes, fails, or `until` passes.
    async fn drive(&mut self, until: Instant) -> Step;
}
