//! `delta` frames to timeline events. The host passes the engine's own event through
//! untouched, so each family brings its own mapping, kept apart from the transport.

use serde_json::Value;

use crate::agents::driver::EngineEvent;
use crate::agents::driver::acp::UpdateMapper;
use crate::agents::driver::claude::SdkMapper;

use super::cursor_delta::CursorMapper;

pub enum DeltaMapper {
    /// The Claude Agent SDK's messages, mapped as the CLI driver maps stream-json.
    Claude(Box<SdkMapper>),
    /// ACP's `session/update` object, mapped by the Rust ACP driver's own mapper.
    Acp(UpdateMapper),
    /// The Cursor SDK's `InteractionUpdate` object.
    Cursor(CursorMapper),
    /// The fake family's `{type:"text", text}`, for the transport tests.
    #[cfg(test)]
    Fake { open: bool },
}

impl DeltaMapper {
    /// The mapping for a family, or `None` for one this app cannot map yet (the run is refused).
    pub fn for_family(family: &str) -> Option<Self> {
        match family {
            "claude" => Some(Self::Claude(Box::default())),
            "acp" => Some(Self::Acp(UpdateMapper::default())),
            "cursor" => Some(Self::Cursor(CursorMapper::default())),
            #[cfg(test)]
            "fake" => Some(Self::Fake { open: false }),
            _ => None,
        }
    }

    pub fn map(&mut self, update: &Value) -> Vec<EngineEvent> {
        match self {
            Self::Claude(mapper) => mapper.map(update),
            Self::Acp(mapper) => mapper.on_update(update),
            Self::Cursor(mapper) => mapper.map(update),
            #[cfg(test)]
            Self::Fake { open } => {
                use orbyn_contracts::AgUiEvent;
                let mut events = Vec::new();
                if !*open {
                    *open = true;
                    events.push(
                        AgUiEvent::TextMessageStart {
                            message_id: "fake".into(),
                            role: "assistant".into(),
                        }
                        .into(),
                    );
                }
                if let Some(text) = update.get("text").and_then(Value::as_str) {
                    events.push(
                        AgUiEvent::TextMessageContent {
                            message_id: "fake".into(),
                            delta: text.to_owned(),
                        }
                        .into(),
                    );
                }
                events
            }
        }
    }

    /// What closes when the run ends.
    pub fn finish(&mut self) -> Vec<EngineEvent> {
        match self {
            Self::Claude(mapper) => mapper.finish(),
            Self::Acp(mapper) => mapper.close(),
            Self::Cursor(mapper) => mapper.finish(),
            #[cfg(test)]
            Self::Fake { open } => {
                if std::mem::take(open) {
                    vec![
                        orbyn_contracts::AgUiEvent::TextMessageEnd {
                            message_id: "fake".into(),
                        }
                        .into(),
                    ]
                } else {
                    Vec::new()
                }
            }
        }
    }
}
