//! Finding the address of a page's icon. The viewer fetches the bytes itself, through the restricted image proxy.

use serde_json::Value;

/// Discover only; the viewer fetches bytes through the restricted image proxy.
pub(super) const FAVICON: &str = r#"(() => {
    const icon = document.querySelector('link[rel~="icon"]');
    return { page_url: location.href, icon_url: icon ? icon.href : new URL('/favicon.ico', location.href).href };
})()"#;

pub(super) fn favicon_url(value: &Value, page_url: &str) -> Option<String> {
    if value.get("page_url")?.as_str()? != page_url {
        return None;
    }
    let raw = value.get("icon_url")?.as_str()?;
    if raw.len() > 4096 {
        return None;
    }
    let mut url = reqwest::Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return None;
    }
    url.set_fragment(None);
    Some(url.to_string())
}
