//! What an agent reads from a page: the element under a point, and a bounded snapshot of what is visible.

use super::{PreviewManager, lock};
use crate::error::{CoreError, Result};
use orbyn_contracts::PickedElement;
use serde_json::{Value, json};

/// A snapshot's bounds: how many elements an agent may read, and how long one
/// name, value, selector, address or text sample may be. They hold in the page and
/// again here, so a page that answers more than asked still stays small.
pub(super) const MAX_SNAPSHOT_ELEMENTS: usize = 300;

pub(super) const DEFAULT_SNAPSHOT_ELEMENTS: usize = 150;

pub(super) const MAX_SNAPSHOT_TEXT: usize = 4_000;

pub(super) const MAX_NAME_CHARS: usize = 120;

pub(super) const MAX_VALUE_CHARS: usize = 120;

pub(crate) const MAX_SELECTOR_CHARS: usize = 300;

pub(super) const MAX_ADDRESS_CHARS: usize = 2_000;

/// Reads the element under a point: its tag, a selector that finds it again, and a
/// bounded sample of what it holds.
pub(super) const PICK_ELEMENT: &str = r"(() => {
  const el = document.elementFromPoint(__X__, __Y__);
  if (!el) return null;
  const path = [];
  for (let node = el; node && node.nodeType === 1 && path.length < 5; node = node.parentElement) {
    if (node.id) { path.unshift(`#${CSS.escape(node.id)}`); break; }
    const same = [...(node.parentElement?.children ?? [])].filter((other) => other.localName === node.localName);
    path.unshift(same.length > 1 ? `${node.localName}:nth-of-type(${same.indexOf(node) + 1})` : node.localName);
  }
  return {
    tag: el.localName,
    selector: path.join(' > '),
    text: (el.innerText ?? '').trim().slice(0, 500),
    html: el.outerHTML.slice(0, 1000),
  };
})()";

/// Reads visible semantic elements with bounded names, values and page text.
/// Selectors stay whole and must uniquely identify their element.
pub(super) const SNAPSHOT_PAGE: &str = r"(() => {
  const LIMIT = __MAX_ELEMENTS__;
  const TEXT_LIMIT = __MAX_TEXT__;
  const NAME_LIMIT = 120;
  const VALUE_LIMIT = 120;
  const SELECTOR_LIMIT = 300;
  const cut = (value, limit) => {
    const text = (value || '').trim().slice(0, limit);
    return /[\uD800-\uDBFF]$/.test(text) ? text.slice(0, -1) : text;
  };
  const selectorOf = (el) => {
    const path = [];
    for (let node = el; node && node.nodeType === 1; node = node.parentElement) {
      const id = node.id ? `#${CSS.escape(node.id)}` : '';
      if (id && document.querySelectorAll(id).length === 1) {
        path.unshift(id);
        break;
      }
      const same = [...(node.parentElement ? node.parentElement.children : [])]
        .filter((other) => other.localName === node.localName);
      const tag = CSS.escape(node.localName);
      path.unshift(same.length > 1
        ? `${tag}:nth-of-type(${same.indexOf(node) + 1})`
        : tag);
    }
    const selector = path.join(' > ');
    if (selector.length > SELECTOR_LIMIT) return null;
    const matches = document.querySelectorAll(selector);
    return matches.length === 1 && matches[0] === el ? selector : null;
  };
  const roleOf = (el) => {
    const type = (el.getAttribute('type') || 'text').toLowerCase();
    switch (el.localName) {
      case 'button': return 'button';
      case 'a': return el.hasAttribute('href') ? 'link' : '';
      case 'textarea': return 'textbox';
      case 'select': return 'combobox';
      case 'img': return 'img';
      case 'input':
        if (type === 'hidden') return '';
        if (type === 'checkbox') return 'checkbox';
        if (type === 'radio') return 'radio';
        if (['button', 'submit', 'reset', 'image'].includes(type)) return 'button';
        return 'textbox';
      default:
        if (el.isContentEditable) return 'textbox';
        return (el.getAttribute('role') || '').split(/\s+/)[0];
    }
  };
  const inner = (el) => (el.innerText != null ? el.innerText : el.textContent) || '';
  const nameOf = (el) => {
    const labelledBy = (el.getAttribute('aria-labelledby') || '').trim().split(/\s+/)
      .map((id) => document.getElementById(id)?.textContent || '').join(' ');
    const referenced = cut(labelledBy.replace(/\s+/g, ' '), NAME_LIMIT);
    if (referenced) return referenced;
    const label = cut(el.getAttribute('aria-label'), NAME_LIMIT);
    if (label) return label;
    const associated = cut(Array.from(el.labels || []).map((label) => {
      const copy = label.cloneNode(true);
      copy.querySelectorAll('input,select,textarea,button').forEach((control) => control.remove());
      return copy.textContent || '';
    }).join(' ').replace(/\s+/g, ' '), NAME_LIMIT);
    if (associated) return associated;
    const shown = ['input', 'select', 'textarea'].includes(el.localName)
      ? '' : cut(inner(el).replace(/\s+/g, ' '), NAME_LIMIT);
    if (shown) return shown;
    return cut(
      el.getAttribute('placeholder') || el.getAttribute('title') || el.getAttribute('alt'),
      NAME_LIMIT,
    );
  };
  const elements = [];
  let truncated = false;
  for (const el of document.querySelectorAll('body *')) {
    const style = getComputedStyle(el);
    if (style.visibility === 'hidden' || style.visibility === 'collapse') continue;
    const box = el.getBoundingClientRect();
    if (!el.getClientRects().length || box.width <= 0 || box.height <= 0) continue;
    const role = roleOf(el);
    if (!role) continue;
    if (elements.length >= LIMIT) { truncated = true; break; }
    const selector = selectorOf(el);
    if (!selector) { truncated = true; continue; }
    const found = { role: role, name: nameOf(el), selector: selector };
    if ((role === 'textbox' || role === 'combobox') &&
        !(el.localName === 'input' && el.type === 'password')) {
      const value = typeof el.value === 'string' ? cut(el.value, VALUE_LIMIT) : '';
      if (value) found.value = value;
    }
    elements.push(found);
  }
  const body = document.body ? inner(document.body) : '';
  let text = body.slice(0, TEXT_LIMIT);
  if (/[\uD800-\uDBFF]$/.test(text)) text = text.slice(0, -1);
  return {
    url: location.href,
    title: document.title,
    elements: elements,
    text: text,
    truncated: truncated || body.length > TEXT_LIMIT,
  };
})()";

/// A snapshot's element limit: what was asked for, kept inside the cap.
pub(super) fn snapshot_limit(asked: Option<u64>) -> usize {
    asked.map_or(DEFAULT_SNAPSHOT_ELEMENTS, |asked| {
        usize::try_from(asked)
            .unwrap_or(MAX_SNAPSHOT_ELEMENTS)
            .clamp(1, MAX_SNAPSHOT_ELEMENTS)
    })
}

/// At most `limit` characters, counted whole so no cut splits a character.
pub(super) fn capped(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// One string field of a page's answer, or nothing.
pub(super) fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Bounds the page's answer again. An oversized selector is omitted, since
/// shortening a selector could select a different element.
pub(super) fn capped_snapshot(page: &Value, limit: usize) -> Value {
    let mut elements: Vec<Value> = Vec::new();
    let mut truncated = page
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    for entry in page
        .get("elements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if elements.len() >= limit {
            truncated = true;
            break;
        }
        let selector = field(entry, "selector");
        if selector.trim().is_empty() || selector.chars().count() > MAX_SELECTOR_CHARS {
            truncated = true;
            continue;
        }
        let mut element = json!({
            "role": capped(field(entry, "role"), MAX_NAME_CHARS),
            "name": capped(field(entry, "name").trim(), MAX_NAME_CHARS),
            "selector": selector,
        });
        let value = capped(field(entry, "value").trim(), MAX_VALUE_CHARS);
        if !value.is_empty() {
            element["value"] = json!(value);
        }
        elements.push(element);
    }
    let text = field(page, "text").trim();
    if text.chars().count() > MAX_SNAPSHOT_TEXT {
        truncated = true;
    }
    json!({
        "url": capped(field(page, "url"), MAX_ADDRESS_CHARS),
        "title": capped(field(page, "title").trim(), MAX_NAME_CHARS),
        "elements": elements,
        "text": capped(text, MAX_SNAPSHOT_TEXT),
        "truncated": truncated,
    })
}

/// A snapshot expression, with the element limit kept inside the cap.
pub(super) fn snapshot_expression(limit: usize) -> String {
    SNAPSHOT_PAGE
        .replace(
            "__MAX_ELEMENTS__",
            &limit.clamp(1, MAX_SNAPSHOT_ELEMENTS).to_string(),
        )
        .replace("__MAX_TEXT__", &MAX_SNAPSHOT_TEXT.to_string())
}

impl PreviewManager {
    /// Reads the element at a point in the page, without touching the page.
    pub async fn pick(&self, thread_id: &str, id: &str, x: f64, y: f64) -> Result<PickedElement> {
        let tab = self.tab(thread_id, id)?;
        let zoom = lock(&tab.info).zoom;
        let expression = PICK_ELEMENT
            .replace("__X__", &(x / zoom).to_string())
            .replace("__Y__", &(y / zoom).to_string());
        let value = tab.session.lock().await.read(&expression).await?;
        serde_json::from_value(value).map_err(|_| CoreError::NotFound("element".into()))
    }

    /// Reads the page's visible controls and a sample of its text. Both are capped
    /// in the page and here, so a snapshot of a busy page still fits in an answer.
    pub async fn snapshot(
        &self,
        thread_id: &str,
        id: &str,
        max_elements: Option<u64>,
    ) -> Result<Value> {
        let limit = snapshot_limit(max_elements);
        let tab = self.tab(thread_id, id)?;
        let page = tab
            .session
            .lock()
            .await
            .read(&snapshot_expression(limit))
            .await?;
        Ok(capped_snapshot(&page, limit))
    }
}
