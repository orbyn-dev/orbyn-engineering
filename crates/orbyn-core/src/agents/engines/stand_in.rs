//! A stand-in engine for tests of the rules around an engine's modes (built only for tests, `test-support`): it completes every
//! run at once and records the permission mode each run was given. It runs no process and talks to no engine.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orbyn_contracts::{EngineCapabilities, EngineTier, PermissionMode, SkillDelivery};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::agents::driver::{
    EngineDescriptor, EngineDriver, EngineEvent, EngineOutcome, EngineProbe, RunHandle, RunSpec,
};
use crate::error::Result;

pub struct StandIn {
    id: &'static str,
    modes: Vec<PermissionMode>,
    seen: Arc<Mutex<Vec<PermissionMode>>>,
}

impl StandIn {
    /// An engine named `id` offering `modes`; `seen` collects the mode of each run it is given.
    pub fn new(
        id: &'static str,
        modes: Vec<PermissionMode>,
        seen: Arc<Mutex<Vec<PermissionMode>>>,
    ) -> Arc<Self> {
        Arc::new(Self { id, modes, seen })
    }
}

#[async_trait]
impl EngineDriver for StandIn {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            id: self.id.into(),
            name: if self.id == "cursor" {
                "Cursor".into()
            } else {
                self.id.into()
            },
            tier: EngineTier::Terminal,
            capabilities: EngineCapabilities {
                streaming: true,
                approvals: false,
                resume: true,
                mcp: true,
                sandbox: false,
                managed: true,
                skill_delivery: vec![SkillDelivery::McpSkillServer],
                goal_control: false,
                decline_feedback: false,
                permission_modes: self.modes.clone(),
            },
        }
    }

    async fn probe(&self) -> EngineProbe {
        EngineProbe {
            path: Some("/bin/echo".into()),
            version: Some("1.0.0".into()),
            ..EngineProbe::default()
        }
    }

    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(spec.mode);
        let task = tokio::spawn(async move {
            let reply = orbyn_contracts::AgUiEvent::TextMessageContent {
                message_id: "m".into(),
                delta: "ok".into(),
            };
            let _ = events.send(reply.into()).await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        });
        Ok(RunHandle {
            cancel: CancellationToken::new(),
            task,
            input: None,
            control: None,
        })
    }
}
