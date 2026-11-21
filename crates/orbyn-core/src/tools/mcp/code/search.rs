//! Ranking tool paths for a short query: a plain term match over name, description and argument names.

use super::declare::declaration;
use serde_json::{Value, json};

pub(super) const DEFAULT_LIMIT: usize = 10;
pub(super) const MAX_LIMIT: usize = 50;

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn score(spec: &Value, terms: &[String]) -> usize {
    let name = spec["name"].as_str().unwrap_or_default().to_lowercase();
    let name_words = words(&name);
    let description = words(spec["description"].as_str().unwrap_or_default());
    let args: Vec<String> = spec["inputSchema"]["properties"]
        .as_object()
        .into_iter()
        .flat_map(|p| p.keys().flat_map(|k| words(k)))
        .collect();
    terms
        .iter()
        .map(|term| {
            let in_name = if name_words.contains(term) {
                6
            } else if name.contains(term.as_str()) {
                3
            } else {
                0
            };
            in_name + 2 * usize::from(description.contains(term)) + usize::from(args.contains(term))
        })
        .sum()
}

/// The best `limit` of `specs` for `query`, at the asked `detail`. An empty query lists by name.
pub(super) fn search(specs: &[Value], query: &str, detail: &str, limit: usize) -> Vec<Value> {
    let terms = words(query);
    let mut ranked: Vec<(usize, &Value)> = specs
        .iter()
        .map(|spec| (score(spec, &terms), spec))
        .filter(|(score, _)| terms.is_empty() || *score > 0)
        .collect();
    ranked.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1["name"].as_str().cmp(&b.1["name"].as_str()))
    });
    ranked
        .into_iter()
        .take(limit.clamp(1, MAX_LIMIT))
        .map(|(_, spec)| {
            let path = spec["name"].clone();
            match detail {
                "name" => json!({ "path": path }),
                "signature" => json!({ "path": path, "description": spec["description"], "signature": declaration(spec) }),
                _ => json!({ "path": path, "description": spec["description"] }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs() -> Vec<Value> {
        vec![
            json!({ "name": "orbyn_thread_send", "description": "Send a message to a thread.", "inputSchema": { "properties": { "text": {} } } }),
            json!({ "name": "orbyn_thread_list", "description": "List threads in the project.", "inputSchema": {} }),
            json!({ "name": "orbyn_worktree_status", "description": "Git status of the worktree.", "inputSchema": {} }),
        ]
    }

    fn paths(hits: &[Value]) -> Vec<&str> {
        hits.iter().map(|h| h["path"].as_str().unwrap()).collect()
    }

    #[test]
    fn a_name_match_outranks_a_description_match() {
        let hits = search(&specs(), "send message", "name", 10);
        assert_eq!(paths(&hits), ["orbyn_thread_send"]);
        let hits = search(&specs(), "thread list", "name", 10);
        assert_eq!(paths(&hits), ["orbyn_thread_list", "orbyn_thread_send"]);
    }

    #[test]
    fn detail_picks_what_comes_back() {
        assert!(
            search(&specs(), "status", "name", 5)[0]
                .get("description")
                .is_none()
        );
        assert!(
            search(&specs(), "status", "summary", 5)[0]
                .get("signature")
                .is_none()
        );
        let signature = &search(&specs(), "status", "signature", 5)[0];
        assert!(
            signature["signature"]
                .as_str()
                .unwrap()
                .contains("declare function orbyn_worktree_status(")
        );
    }

    #[test]
    fn nothing_matching_is_an_empty_list_and_an_empty_query_lists_by_name() {
        assert!(search(&specs(), "zebra", "name", 5).is_empty());
        let all = search(&specs(), "", "name", 2);
        assert_eq!(paths(&all), ["orbyn_thread_list", "orbyn_thread_send"]);
    }
}
