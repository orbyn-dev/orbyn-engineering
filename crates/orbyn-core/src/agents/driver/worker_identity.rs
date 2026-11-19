//! The git identity a Crew worker's engine process carries, so a commit it makes can be told from a person's.
//!
//! The variables are set for that process alone, after `host::child_env::for_engine`, so they win over the
//! person's own git config and their shell profile's exports for the commands the engine starts. They are a label,
//! never a boundary: a worker can override them in its own command, and a commit that carries no worker address
//! is read as "unknown", which coverage counts as agent. Only the exact address `worker+<task id>@orbyn.invalid`
//! is ever matched; the engine and model in the display name are for people and are never matched on.

use orbyn_contracts::CrewHash;

/// The domain of every address the app makes up. It cannot be registered (RFC 2606), so no real mailbox shares it.
pub const IDENTITY_DOMAIN: &str = "orbyn.invalid";

/// The longest engine or model text kept in a display name.
const NAME_PART_MAX: usize = 64;

/// The commit message trailer the brief asks for. It only corroborates the identity: the app cannot enforce it.
pub const TASK_TRAILER: &str = "Orbyn-Task";

const ADDRESS_PREFIX: &str = "worker+";

/// An engine or model id made safe for a git name: anything outside `[A-Za-z0-9._:/-]` becomes `_`, at most 64
/// characters. An empty value reads `unknown`.
pub fn clean_part(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '-') {
                c
            } else {
                '_'
            }
        })
        .take(NAME_PART_MAX)
        .collect();
    if cleaned.is_empty() {
        "unknown".into()
    } else {
        cleaned
    }
}

/// Whether `id` has the app's task id format (`sha256:` and 64 lowercase hex digits).
pub fn is_task_id(id: &str) -> bool {
    CrewHash::new(id).is_ok()
}

/// The address a worker's commits carry. `None` for text that is not a task id, so no other address can be made.
pub fn worker_address(task_id: &str) -> Option<String> {
    is_task_id(task_id).then(|| format!("{ADDRESS_PREFIX}{task_id}@{IDENTITY_DOMAIN}"))
}

/// The task id an email address names, if it is exactly a worker address with a task id of the app's format.
/// Case matters: nothing is normalised, so a look-alike never matches. Whether the task exists is the caller's check.
pub fn task_of_address(email: &str) -> Option<&str> {
    let id = email
        .strip_prefix(ADDRESS_PREFIX)?
        .strip_suffix(IDENTITY_DOMAIN)?
        .strip_suffix('@')?;
    is_task_id(id).then_some(id)
}

/// The four variables for one worker run: author and committer name and email. Empty for text that is not a task id.
pub fn worker_env(task_id: &str, engine: &str, model: Option<&str>) -> Vec<(String, String)> {
    let Some(address) = worker_address(task_id) else {
        return Vec::new();
    };
    let name = format!(
        "Orbyn Crew worker ({}/{})",
        clean_part(engine),
        clean_part(model.unwrap_or("default"))
    );
    ["AUTHOR", "COMMITTER"]
        .into_iter()
        .flat_map(|role| {
            [
                (format!("GIT_{role}_NAME"), name.clone()),
                (format!("GIT_{role}_EMAIL"), address.clone()),
            ]
        })
        .collect()
}

/// `base` followed by `identity`, as the borrowed pairs a spawn takes. The identity is last, so it wins.
pub fn with_identity<'a>(
    base: &[(&'a str, &'a str)],
    identity: &'a [(String, String)],
) -> Vec<(&'a str, &'a str)> {
    base.iter()
        .copied()
        .chain(identity.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> String {
        format!("sha256:{}", "ab".repeat(32))
    }

    #[test]
    fn a_worker_run_gets_author_and_committer_with_the_task_address() {
        let env = worker_env(&id(), "claude", Some("sonnet-4"));
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        let address = format!("worker+{}@orbyn.invalid", id());
        for role in ["AUTHOR", "COMMITTER"] {
            assert_eq!(get(&format!("GIT_{role}_EMAIL")), Some(address.as_str()));
            assert_eq!(
                get(&format!("GIT_{role}_NAME")),
                Some("Orbyn Crew worker (claude/sonnet-4)")
            );
        }
        assert_eq!(env.len(), 4);
    }

    #[test]
    fn engine_and_model_are_cleaned_and_capped() {
        assert_eq!(clean_part("a b<c>\n\"d\""), "a_b_c___d_");
        assert_eq!(clean_part("ok/Model-1.2:x_y"), "ok/Model-1.2:x_y");
        assert_eq!(clean_part(&"x".repeat(200)).len(), 64);
        assert_eq!(clean_part(""), "unknown");
        let env = worker_env(&id(), "a>b", None);
        assert!(
            env.iter()
                .any(|(_, v)| v == "Orbyn Crew worker (a_b/default)")
        );
    }

    #[test]
    fn only_a_task_id_makes_an_address_and_only_an_exact_address_matches() {
        assert!(worker_env("not-an-id", "claude", None).is_empty());
        assert!(worker_address("sha256:ABC").is_none());
        let address = worker_address(&id()).unwrap();
        assert_eq!(task_of_address(&address), Some(id().as_str()));
        for bad in [
            format!("Worker+{}@orbyn.invalid", id()),
            format!("worker+{}@orbyn.invalid ", id()),
            format!("worker+{}@orbyn.invalid.evil.com", id()),
            format!("worker+{}@ORBYN.INVALID", id()),
            format!("x.worker+{}@orbyn.invalid", id()),
            format!("worker+{}@example.com", id()),
            format!("worker+{}@orbyn.invalid", id().to_uppercase()),
            "worker+sha256:abc@orbyn.invalid".to_owned(),
            "worker+@orbyn.invalid".to_owned(),
            String::new(),
        ] {
            assert_eq!(task_of_address(&bad), None, "{bad}");
        }
    }

    #[test]
    fn the_identity_comes_last_so_it_wins() {
        let identity = worker_env(&id(), "codex", None);
        let all = with_identity(&[("A", "1")], &identity);
        assert_eq!(all[0], ("A", "1"));
        assert_eq!(all.last().unwrap().0, "GIT_COMMITTER_EMAIL");
    }
}
