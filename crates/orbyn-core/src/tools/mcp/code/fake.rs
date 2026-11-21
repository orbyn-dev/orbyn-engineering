//! A scripted executor for tests: each line of the "script" is `call <tool>` (through the host),
//! `sleep <ms>` or `return`. `SYNTAX` anywhere fails the parse.

use async_trait::async_trait;
use orbyn_code_sandbox::{CodeExecutor, ErrorKind, Limits, Run, RunError, RunSeed, Step, ToolHost};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) struct Fake;

struct FakeRun {
    lines: Vec<String>,
    host: Arc<dyn ToolHost>,
}

#[async_trait]
impl CodeExecutor for Fake {
    async fn prepare(
        &self,
        source: &str,
        _limits: Limits,
        _seed: RunSeed,
        host: Arc<dyn ToolHost>,
    ) -> Result<Box<dyn Run>, RunError> {
        if source.contains("SYNTAX") {
            return Err(RunError {
                kind: ErrorKind::Syntax,
                message: "bad".into(),
                line: Some(1),
                code: None,
            });
        }
        Ok(Box::new(FakeRun {
            lines: source.lines().map(str::to_owned).collect(),
            host,
        }))
    }
}

#[async_trait]
impl Run for FakeRun {
    async fn drive(&mut self, _until: Instant) -> Step {
        let mut results: Vec<Value> = Vec::new();
        for line in &self.lines {
            let mut words = line.split_whitespace();
            match (words.next(), words.next()) {
                (Some("call"), Some(tool)) => match self.host.call(tool, json!({})).await {
                    Ok(value) => results.push(value),
                    Err(failure) => {
                        let error = RunError {
                            kind: ErrorKind::Runtime,
                            message: failure.message,
                            line: None,
                            code: Some(failure.code),
                        };
                        return Step::Failed {
                            error,
                            logs: vec![],
                            truncated: false,
                        };
                    }
                },
                (Some("sleep"), Some(ms)) => {
                    tokio::time::sleep(Duration::from_millis(ms.parse().unwrap_or(0))).await;
                }
                _ => {}
            }
        }
        Step::Done {
            result: json!(results),
            logs: vec!["ran".into()],
            truncated: false,
        }
    }
}
