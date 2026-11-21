//! The Agent SDK's messages are the CLI's stream-json frames, so the engine host's `delta`
//! frames for Claude go through the same mapper the CLI driver uses. Only the timeline
//! comes from here: whether and when the run ends is the host's `runDone`, which has
//! already applied the run-end contract (docs/engine-run-contract.md), so the mapper's own
//! outcome and hold are dropped.

use serde_json::Value;

use super::events::StreamMapper;
use crate::agents::driver::EngineEvent;

#[derive(Default)]
pub(crate) struct SdkMapper(StreamMapper);

impl SdkMapper {
    /// The events one SDK message makes. Anything the mapper holds back stays in it.
    pub(crate) fn map(&mut self, message: &Value) -> Vec<EngineEvent> {
        let events = self.0.on_frame(message);
        self.0.outcome = None;
        self.0.held = None;
        events
    }

    /// What closes at the end of a run: open blocks, workflows and background work the
    /// engine still ran, each said to have stopped.
    pub(crate) fn finish(&mut self) -> Vec<EngineEvent> {
        let mut events = self.0.close_open_blocks();
        events.extend(self.0.workflows.end_all());
        events.extend(self.0.end_background());
        events
    }
}
