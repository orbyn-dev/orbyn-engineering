//! Reads accessible text through a pure tree interface. No OS calls live here, so every target compiles
//! and tests the collector; the macOS reader in `ax` supplies the nodes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// The most nodes one walk may queue while it is still visiting; a wide node would otherwise
    /// hold up to `MAX_CHILDREN` nodes per visit before the node cap stops it.
    pub max_queued: usize,
    pub deadline: Instant,
    /// Set by the capture flow when it gives up, so a read already running stops early instead of
    /// outliving the capture that started it.
    pub stop: Arc<AtomicBool>,
}

impl Budget {
    pub fn new() -> Self {
        Self {
            max_nodes: 5_000,
            max_depth: 40,
            max_queued: 5_000,
            deadline: Instant::now() + Duration::from_secs(2),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether the wall-clock budget has run out.
    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    /// Whether the capture flow that started this read has given up.
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
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
/// A secure field contributes nothing and hides its whole subtree. The walk stops early once the
/// text budget is spent, the child queue would pass its own cap, the deadline passes or the
/// capture flow asks it to stop.
pub fn collect<T: Tree>(tree: &T, root: &T::Node, budget: &Budget) -> Option<String> {
    let mut stack = vec![Visit::Borrowed(root, 0)];
    let mut lines: Vec<String> = Vec::new();
    let mut chars = 0_usize;
    let mut visited = 0_usize;

    'walk: while let Some(visit) = stack.pop() {
        let depth = match &visit {
            Visit::Borrowed(_, depth) | Visit::Owned(_, depth) => *depth,
        };
        if depth > budget.max_depth {
            continue;
        }
        if visited >= budget.max_nodes || budget.expired() || budget.stopped() {
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
            // The cap is enforced while walking, so a window with many long lines cannot fill the
            // process with text that would only be cut away at the end.
            let width = line.chars().count() + usize::from(!lines.is_empty());
            lines.push(line);
            chars += width;
            if chars >= MAX_ACCESSIBLE_TEXT_CHARS {
                break;
            }
        }
        if depth >= budget.max_depth {
            continue;
        }
        let child_depth = depth + 1;
        for child in tree.children(node).into_iter().rev() {
            // Bound the queue too, not just the visits: a wide node would otherwise retain up to
            // `MAX_CHILDREN` nodes per visit before the node cap stops the walk.
            if stack.len() >= budget.max_queued {
                break 'walk;
            }
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

/// The first title that matches `wanted` exactly. An empty `wanted` never matches.
pub fn pick_window_index(titles: &[String], wanted: &str) -> Option<usize> {
    if wanted.is_empty() {
        return None;
    }
    titles.iter().position(|title| title == wanted)
}

/// What window of the captured app the reader should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowChoice {
    /// The window at this index in `AXWindows` has the captured window's exact title.
    Exact(usize),
    /// No title matched, but the capture was of the app's focused window, so that window is it.
    Focused,
    /// No title matched and the capture was not of the focused window, so no window may be read:
    /// another window of the same app holds different text.
    Skip,
}

/// Chooses the window to read: an exact title match always wins; with no match, only a capture of
/// the focused window may fall back to it.
pub fn window_choice(titles: &[String], wanted: &str, captured_is_focused: bool) -> WindowChoice {
    if let Some(index) = pick_window_index(titles, wanted) {
        return WindowChoice::Exact(index);
    }
    if captured_is_focused {
        WindowChoice::Focused
    } else {
        WindowChoice::Skip
    }
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
    use std::cell::Cell;

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
        Budget::new()
    }

    #[test]
    fn the_default_budget_bounds_the_walk() {
        let budget = Budget::default();
        assert_eq!(budget.max_nodes, 5_000);
        assert_eq!(budget.max_depth, 40);
        assert_eq!(budget.max_queued, 5_000);
        assert!(budget.deadline > Instant::now());
        assert!(!budget.stopped());
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

    /// A tree that counts what the collector asks it for, so a bound can be asserted from outside.
    struct Counted<'a> {
        arena: &'a Arena,
        fields: Cell<usize>,
        children: Cell<usize>,
    }

    impl Tree for Counted<'_> {
        type Node = usize;

        fn fields(&self, node: &usize) -> Fields {
            self.fields.set(self.fields.get() + 1);
            self.arena.nodes[*node].fields.clone()
        }

        fn children(&self, node: &usize) -> Vec<usize> {
            self.children.set(self.children.get() + 1);
            self.arena.nodes[*node].children.clone()
        }
    }

    #[test]
    fn the_walk_stops_at_the_character_cap_instead_of_reading_every_node() {
        let mut arena = Arena::default();
        let mut root = arena.add(text(&format!("root {}", "x".repeat(5_000)), "", ""), &[]);
        for index in 0..39 {
            let value = format!("node {index} {}", "y".repeat(5_000));
            root = arena.add(text(&value, "", ""), &[root]);
        }
        let tree = Counted {
            arena: &arena,
            fields: Cell::new(0),
            children: Cell::new(0),
        };
        let text = collect(&tree, &root, &budget()).expect("the first nodes are kept");
        assert_eq!(text.chars().count(), MAX_ACCESSIBLE_TEXT_CHARS);
        assert!(
            tree.fields.get() <= 10,
            "read {} nodes before the 32,000-character cap",
            tree.fields.get()
        );
        assert!(tree.children.get() <= 10);
    }

    #[test]
    fn the_walk_stops_when_the_child_queue_is_full() {
        struct Wide {
            fields: Cell<usize>,
            children: Cell<usize>,
        }

        impl Tree for Wide {
            type Node = usize;

            fn fields(&self, _node: &usize) -> Fields {
                self.fields.set(self.fields.get() + 1);
                text("node", "", "")
            }

            fn children(&self, node: &usize) -> Vec<usize> {
                self.children.set(self.children.get() + 1);
                (1..=500)
                    .map(|index| node.wrapping_mul(1_000_003).wrapping_add(index))
                    .collect()
            }
        }

        let tree = Wide {
            fields: Cell::new(0),
            children: Cell::new(0),
        };
        let _ = collect(&tree, &0, &budget());
        assert!(
            tree.fields.get() < 100,
            "visited {} nodes while the child queue grew past its cap",
            tree.fields.get()
        );
        assert!(tree.children.get() < 100);
    }

    #[test]
    fn the_budget_reports_expiry_and_stop() {
        let budget = Budget::new();
        assert!(!budget.expired());
        assert!(!budget.stopped());
        budget
            .stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(budget.stopped());
        let expired = Budget {
            deadline: Instant::now(),
            ..Budget::new()
        };
        assert!(expired.expired());
    }

    #[test]
    fn a_stopped_budget_returns_none() {
        let mut arena = Arena::default();
        let root = arena.add(text("text", "", ""), &[]);
        let budget = budget();
        budget
            .stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(collect(&arena, &root, &budget), None);
    }

    #[test]
    fn a_window_choice_needs_a_title_match_or_a_focused_capture() {
        let titles = vec!["Notes".to_owned(), "Mail".to_owned()];

        assert_eq!(
            window_choice(&titles, "Notes", false),
            WindowChoice::Exact(0)
        );
        assert_eq!(window_choice(&titles, "Mail", true), WindowChoice::Exact(1));
        assert_eq!(window_choice(&titles, "notes", true), WindowChoice::Focused);
        assert_eq!(window_choice(&titles, "notes", false), WindowChoice::Skip);
        assert_eq!(window_choice(&titles, "", true), WindowChoice::Focused);
        assert_eq!(window_choice(&titles, "", false), WindowChoice::Skip);
        assert_eq!(window_choice(&[], "Notes", false), WindowChoice::Skip);
    }
}
