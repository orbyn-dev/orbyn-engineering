//! The vendor's own install page for an engine that is not installed. Orbyn installs no
//! native engine itself: before the program exists no installer provably owns it, so the
//! only honest step is the vendor's instructions. This is a fixed list of https addresses
//! shown as links. It is not a command source: nothing here is ever run, and a client
//! cannot add to it. An engine missing from the list has no link.

use orbyn_contracts::VendorPage;

/// Engine id and its official install page. Addresses are the vendor's own; Engineering
/// confirms each one before the beta (see the pull request).
const PAGES: &[(&str, &str)] = &[
    ("claude", "https://code.claude.com/docs/en/setup"),
    ("codex", "https://developers.openai.com/codex/cli"),
    ("cursor", "https://cursor.com/cli"),
    ("opencode", "https://opencode.ai"),
    ("cline", "https://cline.bot"),
    ("copilot", "https://github.com/features/copilot/cli"),
    ("antigravity", "https://antigravity.google"),
];

pub fn vendor_page(engine: &str) -> Option<VendorPage> {
    let (_, url) = PAGES.iter().find(|(id, _)| *id == engine)?;
    let parsed = reqwest::Url::parse(url).ok()?;
    Some(VendorPage {
        url: (*url).to_owned(),
        domain: parsed.host_str()?.trim_start_matches("www.").to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_address_is_https_with_no_credentials_or_query() {
        for (id, url) in PAGES {
            let parsed = reqwest::Url::parse(url).unwrap();
            assert_eq!(parsed.scheme(), "https", "{id}");
            assert!(
                parsed.username().is_empty() && parsed.password().is_none(),
                "{id}"
            );
            assert!(
                parsed.query().is_none() && parsed.fragment().is_none(),
                "{id}"
            );
            assert!(vendor_page(id).is_some(), "{id}");
        }
    }

    #[test]
    fn the_domain_is_the_host_without_www() {
        assert_eq!(vendor_page("cursor").unwrap().domain, "cursor.com");
        assert_eq!(vendor_page("claude").unwrap().domain, "code.claude.com");
        assert!(vendor_page("not-an-engine").is_none());
    }
}
