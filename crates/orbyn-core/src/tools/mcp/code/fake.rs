//! A scripted executor for tests: each line of the "script" is `call <tool> [args]` (through the
//! host), `try <tool> [args]` (the same, but a failure is collected and the script goes on),
//! `sleep <ms>` or `return`. `args` is one JSON object; `SYNTAX` anywhere fails the parse.

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
            let mut words = line.trim().splitn(3, char::is_whitespace);
            let (verb, tool, rest) = (words.next(), words.next(), words.next());
            match (verb, tool, rest) {
                (Some(verb @ ("call" | "try")), Some(tool), rest) => {
                    let args = rest.filter(|text| !text.trim().is_empty()).map_or_else(
                        || json!({}),
                        |text| {
                            serde_json::from_str(text).unwrap_or_else(|why| {
                                panic!("bad JSON arguments in {line:?}: {why}")
                            })
                        },
                    );
                    match (verb, self.host.call(tool, args).await) {
                        ("try", Err(failure)) => results.push(json!({
                            "code": failure.code,
                            "message": failure.message,
                        })),
                        (_, Ok(value)) => results.push(value),
                        (_, Err(failure)) => {
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
                    }
                }
                (Some("sleep"), Some(ms), _) => {
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
