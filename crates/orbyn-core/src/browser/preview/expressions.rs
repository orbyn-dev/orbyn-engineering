//! What an agent asks a page to do: click, type, scroll, run an expression or wait, each within bounds.

use super::{PreviewAct, PreviewAction, PreviewManager, lock};
use crate::error::{CoreError, Result};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// What an agent may ask a page to do at once: one typed string, one wheel of a
/// sane size, one expression, and one waited-for string.
pub(crate) const MAX_TYPE_CHARS: usize = 4_000;

pub(crate) const DEFAULT_SCROLL: f64 = 400.0;

/// Far more than a page can scroll, so a wheel event stays a wheel event.
pub(crate) const MAX_SCROLL: f64 = 100_000.0;

pub(super) const MAX_EXPRESSION_CHARS: usize = 2_000;

pub(super) const MAX_RESULT_BYTES: usize = 20_000;

/// A wait polls 150 ms apart and gives up inside the 45 seconds a tool has to
/// answer, with the round count holding even if the clock stalls.
pub(super) const DEFAULT_WAIT_MS: u64 = 5_000;

pub(super) const MAX_WAIT_MS: u64 = 20_000;

pub(crate) const MAX_WAIT_TEXT_CHARS: usize = 200;

pub(super) const WAIT_POLL: Duration = Duration::from_millis(150);

pub(super) const WAIT_ROUNDS: u32 = 140;

/// Focuses one element and puts the caret at its end, so the text an agent types
/// appends to what the field already holds.
pub(super) const FOCUS_ELEMENT: &str = r"(() => {
  const el = document.querySelector(__SELECTOR__);
  if (!el) return false;
  el.focus();
  if (typeof el.setSelectionRange === 'function' && typeof el.value === 'string') {
    try { el.setSelectionRange(el.value.length, el.value.length); } catch (unusable) {}
  }
  return document.activeElement === el;
})()";

/// The point a wheel event goes to: an element's centre after it is brought into
/// view, or the middle of the viewport when the page is the target.
pub(super) const SCROLL_POINT: &str = r"(() => {
  const selector = __SELECTOR__;
  let target = null;
  if (selector !== null) {
    target = document.querySelector(selector);
    if (!target) return null;
    target.scrollIntoView({ block: 'center' });
  }
  const box = target
    ? target.getBoundingClientRect()
    : { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight };
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
})()";

/// A wait polls one boolean, so each look is a round trip of about a word.
pub(super) const WAIT_SELECTOR: &str = "document.querySelector(__SELECTOR__) !== null";

pub(super) const WAIT_TEXT: &str =
    "(() => (document.body ? document.body.innerText : '').includes(__TEXT__))()";

/// A selector the page cannot parse is refused once, before the wait starts.
pub(super) const WAIT_SELECTOR_PARSES: &str = r"(() => {
  try { document.querySelector(__SELECTOR__); return true; } catch (unreadable) { return false; }
})()";

/// A wait's length: five seconds unless asked for more, never past the cap.
pub(super) fn wait_timeout(asked: Option<u64>) -> Duration {
    Duration::from_millis(asked.unwrap_or(DEFAULT_WAIT_MS).clamp(1, MAX_WAIT_MS))
}

/// What an agent gets from an expression: the whole value, or only its size,
/// never a value cut in half.
pub(super) fn bounded_result(value: Value) -> Value {
    let bytes = serde_json::to_string(&value).map_or(usize::MAX, |text| text.len());
    if bytes > MAX_RESULT_BYTES {
        return json!({ "truncated": true, "bytes": bytes });
    }
    json!({ "value": value, "truncated": false })
}

/// An agent's expression, checked against the cap before the page runs it.
pub(super) fn checked_expression(expression: &str) -> Result<&str> {
    if expression.chars().count() > MAX_EXPRESSION_CHARS {
        return Err(CoreError::Invalid(format!(
            "An expression is at most {MAX_EXPRESSION_CHARS} characters."
        )));
    }
    Ok(expression)
}

/// An expression that focuses one element, with the selector quoted as data.
pub(super) fn focus_expression(selector: &str) -> String {
    FOCUS_ELEMENT.replace(
        "__SELECTOR__",
        &Value::String(selector.to_owned()).to_string(),
    )
}

/// An expression for the point a wheel event goes to, for one element or all.
pub(super) fn scroll_expression(selector: Option<&str>) -> String {
    let target = selector.map_or_else(
        || "null".to_owned(),
        |sel| Value::String(sel.to_owned()).to_string(),
    );
    SCROLL_POINT.replace("__SELECTOR__", &target)
}

/// An expression that is true once a selector matches.
pub(super) fn wait_selector_expression(selector: &str) -> String {
    WAIT_SELECTOR.replace(
        "__SELECTOR__",
        &Value::String(selector.to_owned()).to_string(),
    )
}

/// An expression that is true once the page's text holds a string.
pub(super) fn wait_text_expression(text: &str) -> String {
    WAIT_TEXT.replace("__TEXT__", &Value::String(text.to_owned()).to_string())
}

/// An expression that is true when the page can parse a selector.
pub(super) fn parses_expression(selector: &str) -> String {
    WAIT_SELECTOR_PARSES.replace(
        "__SELECTOR__",
        &Value::String(selector.to_owned()).to_string(),
    )
}

impl PreviewManager {
    /// Clicks one element, types into one field or turns the wheel once, so an
    /// agent drives the page no further than the one action it named.
    pub async fn act(&self, thread_id: &str, id: &str, step: &PreviewAct) -> Result<Value> {
        if step.action != PreviewAction::Scroll && step.selector.is_none() {
            return Err(CoreError::Invalid(
                "Name the selector of the element to act on.".into(),
            ));
        }
        let tab = self.tab(thread_id, id)?;
        let mut session = tab.session.lock().await;
        let document = session.document_generation();
        match step.action {
            PreviewAction::Click => {
                let selector = step.selector.as_deref().unwrap_or_default();
                let (x, y) = session.click_point(selector).await?;
                if document == session.document_generation() {
                    tab.pointer("click", x, y);
                }
                Ok(json!({ "clicked": selector }))
            }
            PreviewAction::Type => {
                let selector = step.selector.as_deref().unwrap_or_default();
                if session.read(&focus_expression(selector)).await? != json!(true) {
                    return Err(CoreError::Invalid(format!(
                        "Nothing to type into matches {selector}."
                    )));
                }
                let typed = step.text.as_deref().unwrap_or_default();
                if let Some(recording) = lock(&tab.recording).as_ref() {
                    recording.key("", false);
                }
                session
                    .input("Input.insertText", json!({ "text": typed }))
                    .await?;
                Ok(json!({ "typed": typed }))
            }
            PreviewAction::Scroll => {
                let point = session
                    .read(&scroll_expression(step.selector.as_deref()))
                    .await?;
                let place = |key: &str| point.get(key).and_then(Value::as_f64);
                let (Some(x), Some(y)) = (place("x"), place("y")) else {
                    return Err(CoreError::Invalid(format!(
                        "Nothing to scroll matches {}.",
                        step.selector.as_deref().unwrap_or("this page")
                    )));
                };
                session
                    .input(
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mouseWheel", "x": x, "y": y,
                            "deltaX": 0, "deltaY": step.dy,
                        }),
                    )
                    .await?;
                if document == session.document_generation() {
                    tab.pointer("move", x, y);
                }
                Ok(json!({ "scrolled": step.dy }))
            }
        }
    }

    /// Runs an agent's expression in the page and answers it whole, or only its
    /// size when the value is too big to pass on.
    pub async fn evaluate(&self, thread_id: &str, id: &str, expression: &str) -> Result<Value> {
        let expression = checked_expression(expression)?;
        let tab = self.tab(thread_id, id)?;
        let value = tab.session.lock().await.read(expression).await?;
        Ok(bounded_result(value))
    }

    /// Waits for a selector or a string to show up. Each poll takes the session for
    /// one boolean, so frames keep coming and other tools still run.
    pub async fn wait_for(
        &self,
        thread_id: &str,
        id: &str,
        selector: Option<&str>,
        wanted: Option<&str>,
        timeout_ms: Option<u64>,
    ) -> Result<Value> {
        let tab = self.tab(thread_id, id)?;
        let timeout = wait_timeout(timeout_ms);
        let started = Instant::now();
        let waited = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        // Include session contention and CDP reads, not only the delays between polls.
        let outcome = tokio::time::timeout(timeout, async {
            let expression = match (selector, wanted) {
                (Some(selector), None) => {
                    if tab
                        .session
                        .lock()
                        .await
                        .read(&parses_expression(selector))
                        .await?
                        != json!(true)
                    {
                        return Err(CoreError::Invalid(format!(
                            "{selector} is not a selector a page can read."
                        )));
                    }
                    wait_selector_expression(selector)
                }
                (None, Some(wanted)) => wait_text_expression(wanted),
                _ => {
                    return Err(CoreError::Invalid(
                        "Name a selector or text to wait for, not both.".into(),
                    ));
                }
            };
            for _ in 0..WAIT_ROUNDS {
                let found = tab.session.lock().await.read(&expression).await;
                if found.as_ref().is_ok_and(|value| value == &json!(true)) {
                    return Ok(json!({ "found": true, "waited_ms": waited() }));
                }
                // A page that navigated mid-poll answers an error, which is a miss.
                tokio::time::sleep(WAIT_POLL).await;
            }
            Ok(json!({ "found": false, "waited_ms": waited() }))
        })
        .await;
        outcome.unwrap_or_else(|_| Ok(json!({ "found": false, "waited_ms": waited() })))
    }
}
