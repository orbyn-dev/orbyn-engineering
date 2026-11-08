//! Helpers for the lists of tools and connectors an engine reports. Only names, states and
//! short reasons are kept; an address, token or request body never leaves the engine's reply.

use orbyn_contracts::{EngineTools, ToolEntry, ToolGroup, ToolState};

const REASON_CHARS: usize = 140;
const DETAIL_CHARS: usize = 1500;
const DESCRIPTION_CHARS: usize = 120;

/// The first sentence or line of `text`, cut to `max` characters.
pub(crate) fn short(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let line = line.split_once(". ").map_or(line, |(first, _)| first);
    let mut out: String = line.chars().take(max).collect();
    if line.chars().count() > max {
        out.push('…');
    }
    out
}

pub(crate) fn reason(text: &str) -> String {
    short(text, REASON_CHARS)
}

pub(crate) fn description(text: &str) -> Option<String> {
    let line = short(text, DESCRIPTION_CHARS);
    (!line.is_empty()).then_some(line)
}

pub(crate) fn detail(text: &str) -> String {
    text.chars().take(DETAIL_CHARS).collect()
}

pub(crate) fn entry(name: &str, state: ToolState) -> ToolEntry {
    ToolEntry {
        name: name.to_owned(),
        description: None,
        state,
        reason: None,
        detail: None,
        source: None,
        tools: None,
        sign_in_url: None,
    }
}

pub(crate) fn group(id: &str, items: Vec<ToolEntry>) -> ToolGroup {
    ToolGroup {
        id: id.to_owned(),
        count: u32::try_from(items.len()).unwrap_or(u32::MAX),
        items,
    }
}

/// A group that is listed by count alone, such as Orbyn's own tools.
pub(crate) fn counted(id: &str, count: usize) -> ToolGroup {
    ToolGroup {
        id: id.to_owned(),
        count: u32::try_from(count).unwrap_or(u32::MAX),
        items: Vec::new(),
    }
}

pub(crate) const ALL_GROUPS: &[&str] =
    &["builtin", "orbyn", "mcp", "connectors", "plugins", "skills"];

/// Orders the groups the engine reported and names the ones it did not.
pub(crate) fn assemble(
    engine: &str,
    source: &str,
    mut groups: Vec<ToolGroup>,
    reported: bool,
) -> EngineTools {
    groups.sort_by_key(|group| {
        ALL_GROUPS
            .iter()
            .position(|id| *id == group.id)
            .unwrap_or(usize::MAX)
    });
    let not_reported = ALL_GROUPS
        .iter()
        .filter(|id| !groups.iter().any(|group| group.id == **id))
        .map(|id| (*id).to_owned())
        .collect();
    EngineTools {
        engine: engine.to_owned(),
        source: source.to_owned(),
        checked_at: Some(crate::util::now()),
        groups,
        not_reported,
        error: None,
        reported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cut_to_a_short_plain_line() {
        assert_eq!(short("First line. Second.\nThird", 100), "First line");
        assert_eq!(short(&"x".repeat(200), 10), format!("{}…", "x".repeat(10)));
        assert_eq!(description("   "), None);
    }

    #[test]
    fn groups_come_out_in_display_order_and_the_missing_are_named() {
        let tools = assemble(
            "codex",
            "check",
            vec![counted("plugins", 1), counted("mcp", 2)],
            true,
        );
        let ids: Vec<_> = tools.groups.iter().map(|group| group.id.as_str()).collect();
        assert_eq!(ids, ["mcp", "plugins"]);
        assert_eq!(
            tools.not_reported,
            ["builtin", "orbyn", "connectors", "skills"]
        );
    }
}
