//! Reads accessible text through a pure tree interface. No OS calls live here, so every target compiles
//! and tests the collector; the macOS reader in `ax` supplies the nodes.

use std::time::{Duration, Instant};

pub const MAX_ACCESSIBLE_TEXT_CHARS: usize = 32_000;
const SECURE_ROLE: &str = "AXSecureTextField";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub role: String,
    pub subrole: String,
    pub value: String,
    pub title: String,
    pub description: String,
}

pub trait Tree {
    type Node;

    fn fields(&self, node: &Self::Node) -> Fields;

    fn children(&self, node: &Self::Node) -> Vec<Self::Node>;
}

pub struct Budget {
    pub max_nodes: usize,
    pub max_depth: usize,
    pub deadline: Instant,
}

impl Budget {
    pub fn new() -> Self {
        Self {
            max_nodes: 5_000,
            max_depth: 40,
            deadline: Instant::now() + Duration::from_secs(2),
        }
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

enum Visit<'a, N> {
    Borrowed(&'a N, usize),
    Owned(N, usize),
}

/// Walks the tree depth first, in order, and returns at most [`MAX_ACCESSIBLE_TEXT_CHARS`] characters.
/// A secure field contributes nothing and hides its whole subtree.
pub fn collect<T: Tree>(tree: &T, root: &T::Node, budget: &Budget) -> Option<String> {
    let mut stack = vec![Visit::Borrowed(root, 0)];
    let mut lines: Vec<String> = Vec::new();
    let mut visited = 0_usize;

    while let Some(visit) = stack.pop() {
        let depth = match &visit {
            Visit::Borrowed(_, depth) | Visit::Owned(_, depth) => *depth,
        };
        if depth > budget.max_depth {
            continue;
        }
        if visited >= budget.max_nodes {
            break;
        }
        if Instant::now() >= budget.deadline {
            break;
        }
        visited += 1;

        let node = match &visit {
            Visit::Borrowed(node, _) => *node,
            Visit::Owned(node, _) => node,
        };
        let fields = tree.fields(node);
        if is_secure(&fields) {
            continue;
        }
        if let Some(line) = line(&fields)
            && lines.last().map(String::as_str) != Some(line.as_str())
        {
            lines.push(line);
        }
        if depth >= budget.max_depth {
            continue;
        }
        let child_depth = depth + 1;
        for child in tree.children(node).into_iter().rev() {
            stack.push(Visit::Owned(child, child_depth));
        }
    }

    if lines.is_empty() {
        return None;
    }
    let mut text = lines.join("\n");
    if let Some((end, _)) = text.char_indices().nth(MAX_ACCESSIBLE_TEXT_CHARS) {
        text.truncate(end);
    }
    Some(text)
}

/// The first title that matches `wanted` exactly. An empty `wanted` never matches, so the caller
/// falls back to the focused window.
pub fn pick_window_index(titles: &[String], wanted: &str) -> Option<usize> {
    if wanted.is_empty() {
        return None;
    }
    titles.iter().position(|title| title == wanted)
}

/// The UTF-8 buffer bound for `utf16_units` UTF-16 units: three bytes per unit is the worst case,
/// plus four spare bytes, saturating instead of overflowing.
pub fn utf8_buffer_len(utf16_units: usize) -> usize {
    utf16_units.saturating_mul(3).saturating_add(4)
}

fn is_secure(fields: &Fields) -> bool {
    fields.role == SECURE_ROLE || fields.subrole == SECURE_ROLE
}

fn line(fields: &Fields) -> Option<String> {
    normalized(&fields.value)
        .or_else(|| normalized(&fields.title))
        .or_else(|| normalized(&fields.description))
}

fn normalized(value: &str) -> Option<String> {
    let mut words = value.split_whitespace();
    let first = words.next()?;
    let mut line = String::from(first);
    for word in words {
        line.push(' ');
        line.push_str(word);
    }
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Arena {
        nodes: Vec<Node>,
    }

    struct Node {
        fields: Fields,
        children: Vec<usize>,
    }

    impl Arena {
        fn add(&mut self, fields: Fields, children: &[usize]) -> usize {
            let index = self.nodes.len();
            self.nodes.push(Node {
                fields,
                children: children.to_vec(),
            });
            index
        }
    }

    impl Tree for Arena {
        type Node = usize;

        fn fields(&self, node: &usize) -> Fields {
            self.nodes[*node].fields.clone()
        }

        fn children(&self, node: &usize) -> Vec<usize> {
            self.nodes[*node].children.clone()
        }
    }

    fn text(value: &str, title: &str, description: &str) -> Fields {
        Fields {
            role: "AXStaticText".to_owned(),
            subrole: String::new(),
            value: value.to_owned(),
            title: title.to_owned(),
            description: description.to_owned(),
        }
    }

    fn budget() -> Budget {
        Budget {
            max_nodes: 5_000,
            max_depth: 40,
            deadline: Instant::now() + Duration::from_secs(2),
        }
    }

    #[test]
    fn the_default_budget_bounds_the_walk() {
        let budget = Budget::default();
        assert_eq!(budget.max_nodes, 5_000);
        assert_eq!(budget.max_depth, 40);
        assert!(budget.deadline > Instant::now());
    }

    #[test]
    fn depth_first_order_and_one_line_per_node() {
        let mut arena = Arena::default();
        let grandchild = arena.add(text("grandchild", "ignored title", "ignored text"), &[]);
        let first = arena.add(text("first", "", ""), &[grandchild]);
        let second = arena.add(text("second", "", ""), &[]);
        let root = arena.add(text("root", "", ""), &[first, second]);

        assert_eq!(
            collect(&arena, &root, &budget()).as_deref(),
            Some("root\nfirst\ngrandchild\nsecond")
        );
    }

    #[test]
    fn value_beats_title_beats_description() {
        let mut arena = Arena::default();
        let value = arena.add(text("  value  ", "title", "description"), &[]);
        let title = arena.add(text("", "only title", "description"), &[]);
        let description = arena.add(text(" \t ", "", "only description"), &[]);
        let root = arena.add(text("root", "", ""), &[value, title, description]);

        assert_eq!(
            collect(&arena, &root, &budget()).as_deref(),
            Some("root\nvalue\nonly title\nonly description")
        );
    }

    #[test]
    fn whitespace_is_trimmed_and_collapsed() {
        let mut arena = Arena::default();
        let block = arena.add(text("  hello \t world\nagain  ", "", ""), &[]);
        let heading = arena.add(text("", "  spaced \r\n title  ", ""), &[]);
        let root = arena.add(text("root", "", ""), &[block, heading]);

        assert_eq!(
            collect(&arena, &root, &budget()).as_deref(),
            Some("root\nhello world again\nspaced title")
        );
    }

    #[test]
    fn a_secure_field_and_its_whole_subtree_are_skipped() {
        let mut arena = Arena::default();
        let hidden_grandchild = arena.add(text("hidden grandchild", "", ""), &[]);
        let hidden_child = arena.add(text("hidden child", "", ""), &[hidden_grandchild]);
        let secure_role = Fields {
            role: "AXSecureTextField".to_owned(),
            subrole: String::new(),
            value: "password".to_owned(),
            title: String::new(),
            description: String::new(),
        };
        let secure = arena.add(secure_role, &[hidden_child]);
        let after = arena.add(text("after", "", ""), &[]);
        let root = arena.add(text("root", "", ""), &[secure, after]);

        assert_eq!(
            collect(&arena, &root, &budget()).as_deref(),
            Some("root\nafter")
        );

        let nested_grandchild = arena.add(text("nested hidden", "", ""), &[]);
        let nested_secure = Fields {
            role: "AXGroup".to_owned(),
            subrole: "AXSecureTextField".to_owned(),
            value: "secret".to_owned(),
            title: String::new(),
            description: String::new(),
        };
        let nested = arena.add(nested_secure, &[nested_grandchild]);
        let near_match = Fields {
            role: "AXSecureTextField2".to_owned(),
            subrole: String::new(),
            value: "shown".to_owned(),
            title: String::new(),
            description: String::new(),
        };
        let near = arena.add(near_match, &[]);
        let root = arena.add(text("root", "", ""), &[nested, near]);

        assert_eq!(
            collect(&arena, &root, &budget()).as_deref(),
            Some("root\nshown")
        );
    }

    #[test]
    fn consecutive_duplicate_lines_are_dropped() {
        let mut arena = Arena::default();
        let first = arena.add(text("same", "", ""), &[]);
        let second = arena.add(text("same", "", ""), &[]);
        let other = arena.add(text("other", "", ""), &[]);
        let again = arena.add(text("same", "", ""), &[]);
        let root = arena.add(text("same", "", ""), &[first, second, other, again]);

        assert_eq!(
            collect(&arena, &root, &budget()).as_deref(),
            Some("same\nother\nsame")
        );
    }

    #[test]
    fn the_node_cap_stops_the_walk() {
        let mut arena = Arena::default();
        let mut children = Vec::new();
        for index in 0..5 {
            children.push(arena.add(text(&format!("child {index}"), "", ""), &[]));
        }
        let root = arena.add(text("root", "", ""), &children);
        let capped = Budget {
            max_nodes: 3,
            ..budget()
        };

        assert_eq!(
            collect(&arena, &root, &capped).as_deref(),
            Some("root\nchild 0\nchild 1")
        );
    }

    #[test]
    fn the_depth_cap_skips_deeper_nodes() {
        let mut arena = Arena::default();
        let leaf = arena.add(text("leaf", "", ""), &[]);
        let level_two = arena.add(text("level two", "", ""), &[leaf]);
        let level_one = arena.add(text("level one", "", ""), &[level_two]);
        let root = arena.add(text("root", "", ""), &[level_one]);
        let capped = Budget {
            max_depth: 1,
            ..budget()
        };

        assert_eq!(
            collect(&arena, &root, &capped).as_deref(),
            Some("root\nlevel one")
        );
    }

    #[test]
    fn an_expired_deadline_returns_none() {
        let mut arena = Arena::default();
        let root = arena.add(text("root", "", ""), &[]);
        let expired = Budget {
            deadline: Instant::now(),
            ..budget()
        };

        assert_eq!(collect(&arena, &root, &expired), None);
    }

    #[test]
    fn the_text_is_cut_to_thirty_two_thousand_characters_without_splitting_one() {
        for character in ['é', '🙂'] {
            let mut arena = Arena::default();
            let value = character.to_string().repeat(40_000);
            let root = arena.add(text(&value, "", ""), &[]);
            let text = collect(&arena, &root, &budget()).expect("the long line is kept");
            assert_eq!(text.chars().count(), MAX_ACCESSIBLE_TEXT_CHARS);
            assert!(text.chars().all(|candidate| candidate == character));
        }
    }

    #[test]
    fn an_empty_tree_returns_none() {
        let mut arena = Arena::default();
        let root = arena.add(
            Fields {
                role: "AXGroup".to_owned(),
                ..Fields::default()
            },
            &[],
        );

        assert_eq!(collect(&arena, &root, &budget()), None);
    }

    #[test]
    fn a_window_title_matches_the_first_exact_title() {
        let titles = vec!["Notes".to_owned(), "Mail".to_owned(), "Notes".to_owned()];

        assert_eq!(pick_window_index(&titles, "Notes"), Some(0));
        assert_eq!(pick_window_index(&titles, "Mail"), Some(1));
        assert_eq!(pick_window_index(&titles, "notes"), None);
        assert_eq!(pick_window_index(&titles, "Note"), None);
        assert_eq!(pick_window_index(&titles, " Notes"), None);
        assert_eq!(pick_window_index(&titles, ""), None);
        assert_eq!(pick_window_index(&[], "Notes"), None);
    }

    #[test]
    fn the_utf8_buffer_is_three_bytes_per_utf16_unit_plus_four() {
        assert_eq!(utf8_buffer_len(0), 4);
        assert_eq!(utf8_buffer_len(1), 7);
        assert_eq!(utf8_buffer_len(20_000), 60_004);
        assert_eq!(utf8_buffer_len(usize::MAX), usize::MAX);
        assert_eq!(utf8_buffer_len(usize::MAX / 3 + 1), usize::MAX);
    }
}
