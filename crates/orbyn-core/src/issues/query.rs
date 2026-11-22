//! What a listing asks for: state, label, assignee and text, and nothing else.

use crate::error::{CoreError, Result};
use orbyn_contracts::TrackerKind;
use std::collections::BTreeMap;

pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 100;
const MAX_LABELS: usize = 20;
/// The most the `cursors` value may hold, as the page sends it (docs/issues.md, "Paging").
const MAX_CURSORS_BYTES: usize = 4096;
const MAX_CURSORS: usize = 50;

/// What a listing may emit: at most [`MAX_CURSORS`] and [`MAX_CURSORS_BYTES`] as the JSON the page sends,
/// keeping the first in key order and dropping the rest.
pub(crate) fn capped_cursors(cursors: BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut kept = BTreeMap::new();
    for (key, value) in cursors {
        kept.insert(key, value);
        let fits = kept.len() <= MAX_CURSORS
            && serde_json::to_string(&kept).is_ok_and(|json| json.len() <= MAX_CURSORS_BYTES);
        if !fits {
            kept.pop_last();
            break;
        }
    }
    kept
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateFilter {
    Open,
    Closed,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssigneeFilter {
    Any,
    Me,
    NoOne,
    Login(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueQuery {
    pub tracker: Option<TrackerKind>,
    pub state: StateFilter,
    pub labels: Vec<String>,
    pub assignee: AssigneeFilter,
    pub text: String,
    pub limit: usize,
    /// Where to carry on, per `"<tracker> <scope>"`. Empty for a first page. A source checks that it issued each one.
    pub cursors: BTreeMap<String, String>,
}

impl Default for IssueQuery {
    fn default() -> Self {
        Self {
            tracker: None,
            state: StateFilter::Open,
            labels: Vec::new(),
            assignee: AssigneeFilter::Any,
            text: String::new(),
            limit: DEFAULT_LIMIT,
            cursors: BTreeMap::new(),
        }
    }
}

fn plain(text: &str, max: usize, what: &str) -> Result<()> {
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(CoreError::Invalid(format!(
            "{what} is too long or has control characters."
        )));
    }
    Ok(())
}

impl IssueQuery {
    /// Reads the route's query pairs. Unknown names are refused, so a filter that does not exist is not ignored.
    pub fn from_pairs(pairs: &[(String, String)]) -> Result<Self> {
        let mut query = Self::default();
        for (name, value) in pairs {
            match name.as_str() {
                "tracker" => {
                    query.tracker = Some(crate::store::tracker_of(value).ok_or_else(|| {
                        CoreError::Invalid(
                            "tracker is github, gitlab, azure_devops, forgejo or linear.".into(),
                        )
                    })?);
                }
                "state" => {
                    query.state = match value.as_str() {
                        "open" => StateFilter::Open,
                        "closed" => StateFilter::Closed,
                        "all" => StateFilter::All,
                        _ => {
                            return Err(CoreError::Invalid("state is open, closed or all.".into()));
                        }
                    };
                }
                "label" => {
                    plain(value, 200, "A label")?;
                    if value.trim().is_empty() {
                        return Err(CoreError::Invalid("A label is empty.".into()));
                    }
                    query.labels.push(value.trim().to_owned());
                    if query.labels.len() > MAX_LABELS {
                        return Err(CoreError::Invalid(format!(
                            "Filter by at most {MAX_LABELS} labels."
                        )));
                    }
                }
                "assignee" => {
                    plain(value, 320, "The assignee")?;
                    query.assignee = match value.as_str() {
                        "any" | "" => AssigneeFilter::Any,
                        "me" => AssigneeFilter::Me,
                        "none" => AssigneeFilter::NoOne,
                        login if login.chars().any(char::is_whitespace) => {
                            return Err(CoreError::Invalid(
                                "The assignee is a login, with no spaces.".into(),
                            ));
                        }
                        login => AssigneeFilter::Login(login.to_owned()),
                    };
                }
                "q" => {
                    plain(value, 200, "The search text")?;
                    query.text = value.trim().to_owned();
                }
                "limit" => {
                    let n: usize = value
                        .parse()
                        .map_err(|_| CoreError::Invalid("limit is a number.".into()))?;
                    query.limit = n.clamp(1, MAX_LIMIT);
                }
                "cursors" => {
                    let invalid = || {
                        CoreError::Invalid(
                            "cursors is a JSON object of the cursors the service issued.".into(),
                        )
                    };
                    if value.len() > MAX_CURSORS_BYTES {
                        return Err(invalid());
                    }
                    let cursors: BTreeMap<String, String> =
                        serde_json::from_str(value).map_err(|_| invalid())?;
                    if cursors.len() > MAX_CURSORS
                        || cursors.iter().any(|(k, v)| {
                            k.len() > 300
                                || v.len() > 200
                                || k.chars().chain(v.chars()).any(char::is_control)
                        })
                    {
                        return Err(invalid());
                    }
                    query.cursors = cursors;
                }
                other => {
                    return Err(CoreError::Invalid(format!(
                        "{other} is not an issue filter."
                    )));
                }
            }
        }
        Ok(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn the_four_filters_are_read() {
        let q = IssueQuery::from_pairs(&pairs(&[
            ("tracker", "gitlab"),
            ("state", "all"),
            ("label", "bug"),
            ("label", "ui"),
            ("assignee", "me"),
            ("q", " crash "),
            ("limit", "500"),
        ]))
        .unwrap();
        assert_eq!(q.tracker, Some(TrackerKind::Gitlab));
        assert_eq!(q.state, StateFilter::All);
        assert_eq!(q.labels, ["bug", "ui"]);
        assert_eq!(q.assignee, AssigneeFilter::Me);
        assert_eq!(q.text, "crash");
        assert_eq!(q.limit, MAX_LIMIT);
    }

    #[test]
    fn defaults_are_open_any_and_fifty() {
        let q = IssueQuery::from_pairs(&[]).unwrap();
        assert_eq!(
            (q.state, q.assignee, q.limit),
            (StateFilter::Open, AssigneeFilter::Any, DEFAULT_LIMIT)
        );
    }

    #[test]
    fn cursors_are_a_small_json_object_of_text() {
        let q = IssueQuery::from_pairs(&pairs(&[("cursors", r#"{"gitlab grp/proj":"2.abc"}"#)]))
            .unwrap();
        assert_eq!(q.cursors["gitlab grp/proj"], "2.abc");
        let long = format!(r#"{{"gitlab a/b":"{}"}}"#, "x".repeat(MAX_CURSORS_BYTES));
        for bad in [
            "not json",
            "[]",
            r#"{"a":1}"#,
            r#"{"a":"b\u0001"}"#,
            long.as_str(),
        ] {
            assert!(
                IssueQuery::from_pairs(&pairs(&[("cursors", bad)])).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in [
            ("state", "merged"),
            ("tracker", "jira"),
            ("sort", "created"),
            ("involvement", "authored"),
            ("label", " "),
            ("assignee", "a b"),
            ("limit", "x"),
            ("q", "a\nb"),
        ] {
            assert!(IssueQuery::from_pairs(&pairs(&[bad])).is_err(), "{bad:?}");
        }
        let many: Vec<(String, String)> = (0..21)
            .map(|i| ("label".to_owned(), format!("l{i}")))
            .collect();
        assert!(IssueQuery::from_pairs(&many).is_err());
    }
}
