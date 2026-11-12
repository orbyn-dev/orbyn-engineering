//! The vendor's own install page for an engine that is not installed. Orbyn installs no
//! native engine itself: before the program exists no installer provably owns it, so the
//! only honest step is the vendor's instructions. This is a fixed list of https addresses
//! shown as links. It is not a command source: nothing here is ever run, and a client
//! cannot add to it. An engine missing from the list has no link.

use orbyn_contracts::VendorPage;

/// Engine id, the vendor's install page and the domain shown beside the link. Each address is
/// the final one after its redirects, fetched on 2026-11-12; re-check each before the beta.
const PAGES: &[(&str, &str, &str)] = &[
    (
        "claude",
        "https://code.claude.com/docs/en/setup",
        "code.claude.com",
    ),
    (
        "codex",
        "https://learn.chatgpt.com/docs/codex/cli",
        "learn.chatgpt.com",
    ),
    ("cursor", "https://cursor.com/cli", "cursor.com"),
    ("opencode", "https://opencode.ai", "opencode.ai"),
    (
        "cline",
        "https://docs.cline.bot/usage/cli-overview",
        "docs.cline.bot",
    ),
    (
        "copilot",
        "https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli",
        "docs.github.com",
    ),
];

pub fn vendor_page(engine: &str) -> Option<VendorPage> {
    let (_, url, domain) = PAGES.iter().find(|(id, _, _)| *id == engine)?;
    Some(VendorPage {
        url: (*url).to_owned(),
        domain: (*domain).to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The only hosts a link may point at. Adding one is a reviewed change.
    const ALLOWED_HOSTS: &[&str] = &[
        "code.claude.com",
        "learn.chatgpt.com",
        "cursor.com",
        "opencode.ai",
        "docs.cline.bot",
        "docs.github.com",
    ];

    #[test]
    fn every_address_is_https_on_an_allowed_host_that_is_the_one_shown() {
        for (id, url, domain) in PAGES {
            let parsed = reqwest::Url::parse(url).unwrap();
            assert_eq!(parsed.scheme(), "https", "{id}");
            assert_eq!(
                parsed.host_str(),
                Some(*domain),
                "{id}: the shown domain is not the host"
            );
            assert!(
                ALLOWED_HOSTS.contains(domain),
                "{id}: {domain} is not allowed"
            );
            assert!(
                parsed.username().is_empty() && parsed.password().is_none(),
                "{id}"
            );
            assert!(
                parsed.query().is_none() && parsed.fragment().is_none(),
                "{id}"
            );
            assert_eq!(vendor_page(id).unwrap().domain, *domain);
        }
    }

    #[test]
    fn an_engine_without_a_checked_page_has_no_link() {
        assert!(vendor_page("antigravity").is_none());
        assert!(vendor_page("not-an-engine").is_none());
    }
}
