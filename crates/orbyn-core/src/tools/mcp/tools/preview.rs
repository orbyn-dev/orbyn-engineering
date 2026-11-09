//! The page tools: what a preview action or wait asks for, checked before it reaches the tab.

use super::text;
use crate::app::Core;
use crate::browser::preview::{
    DEFAULT_SCROLL, MAX_SCROLL, MAX_SELECTOR_CHARS, MAX_TYPE_CHARS, MAX_WAIT_TEXT_CHARS,
    PreviewAct, PreviewAction,
};
use crate::error::{CoreError, Result};
use orbyn_contracts::Thread;
use serde_json::Value;

/// `orbyn_preview_act`'s arguments, checked before the page is touched: one known
/// action, a selector where it needs one, and text only where typing uses it.
pub(super) fn page_act(args: &Value) -> Result<PreviewAct> {
    let asked = text(args, "action")?;
    let action = match asked {
        "click" => PreviewAction::Click,
        "type" => PreviewAction::Type,
        "scroll" => PreviewAction::Scroll,
        other => {
            return Err(CoreError::Invalid(format!(
                "{other} is not an action. Use click, type or scroll."
            )));
        }
    };
    let selector = args
        .get("selector")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    let typed = args
        .get("text")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if action != PreviewAction::Scroll && selector.is_none() {
        return Err(CoreError::Invalid(
            "Name the selector of the element to act on.".into(),
        ));
    }
    if action == PreviewAction::Type {
        let Some(typed) = typed.as_ref() else {
            return Err(CoreError::Invalid("Say what text to type.".into()));
        };
        if typed.chars().count() > MAX_TYPE_CHARS {
            return Err(CoreError::Invalid(format!(
                "Text to type is at most {MAX_TYPE_CHARS} characters."
            )));
        }
    }
    let dy = args
        .get("dy")
        .and_then(Value::as_f64)
        .filter(|dy| dy.is_finite())
        .unwrap_or(DEFAULT_SCROLL)
        .clamp(-MAX_SCROLL, MAX_SCROLL);
    Ok(PreviewAct {
        action,
        selector,
        text: typed,
        dy,
    })
}

/// `orbyn_preview_wait_for` waits on exactly one of a selector or a text, and the
/// text it looks for is short enough to check on every poll.
pub(super) fn wait_target(args: &Value) -> Result<(Option<String>, Option<String>)> {
    let field = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    };
    match (field("selector"), field("text")) {
        (Some(selector), None) => {
            if selector.chars().count() > MAX_SELECTOR_CHARS {
                return Err(CoreError::Invalid(format!(
                    "A selector is at most {MAX_SELECTOR_CHARS} characters."
                )));
            }
            Ok((Some(selector), None))
        }
        (None, Some(wanted)) => {
            if wanted.chars().count() > MAX_WAIT_TEXT_CHARS {
                return Err(CoreError::Invalid(format!(
                    "Text to wait for is at most {MAX_WAIT_TEXT_CHARS} characters."
                )));
            }
            Ok((None, Some(wanted)))
        }
        (Some(_), Some(_)) => Err(CoreError::Invalid(
            "Name a selector or text to wait for, not both.".into(),
        )),
        (None, None) => Err(CoreError::Invalid(
            "Name a selector or text to wait for.".into(),
        )),
    }
}

impl Core {
    pub(super) fn own_preview(&self, own: &Thread, args: &Value) -> Result<String> {
        let tab = text(args, "tab_id")?.to_owned();
        if !self
            .previews
            .list(&own.id)
            .iter()
            .any(|view| view.id == tab)
        {
            return Err(CoreError::NotFound("preview".into()));
        }
        Ok(tab)
    }
}
