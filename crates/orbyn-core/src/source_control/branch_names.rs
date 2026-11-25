//! Branch names for new worktrees (parity F25; docs/architecture.md "Text generators").
//!
//! A new worktree gets one branch name, chosen before `git worktree add -b` runs. The
//! static mode keeps `<prefix>/<purpose>`. The semantic mode renders the thread's real
//! title, or else the worktree's purpose, as a lowercase slug and makes it unique against
//! the project's local branches. The custom mode uses the purpose as the complete name.
//! Every name here is deterministic: no engine call is made, so a
//! `branch_name_instructions` preference is not read.

/// The longest generated branch name, in characters.
pub(crate) const MAX_BRANCH_CHARS: usize = 64;
/// How many local branches a name is checked against; a longer list is cut rather than
/// read unbounded.
pub(crate) const MAX_EXISTING_BRANCHES: usize = 10_000;
/// Collision suffixes tried before Git is left to refuse the name.
const MAX_COLLISION_TRIES: u32 = 1_000;

/// How a new worktree's branch gets its name, from the `branch_naming_mode` preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NamingMode {
    /// `<branch_name_prefix>/<purpose>`, the name every worktree used before modes existed.
    #[default]
    Static,
    /// A bounded slug of the thread's real title, or of the purpose when it has none.
    Semantic,
    /// The purpose as the complete branch name.
    Custom,
}

impl NamingMode {
    pub(crate) fn parse(value: &str) -> Self {
        match value {
            "semantic" => Self::Semantic,
            "custom" => Self::Custom,
            _ => Self::Static,
        }
    }
}

/// The naming preferences a new worktree uses.
#[derive(Clone, Debug)]
pub struct Naming {
    /// The mode that decides the name.
    pub mode: NamingMode,
    /// The prefix static and semantic names start with.
    pub prefix: String,
}

impl Default for Naming {
    fn default() -> Self {
        Self {
            mode: NamingMode::Static,
            prefix: "feat".to_owned(),
        }
    }
}

/// The branch a new worktree starts on. `seed` is the thread's real title when it has
/// one; `purpose` is the worktree folder's name and the name of last resort.
pub(crate) fn branch_name(
    naming: &Naming,
    seed: Option<&str>,
    purpose: &str,
    existing: &[String],
) -> String {
    let fallback = static_name(&naming.prefix, purpose);
    match naming.mode {
        NamingMode::Static => fallback,
        NamingMode::Semantic => {
            semantic_name(&naming.prefix, seed, purpose, existing).unwrap_or(fallback)
        }
        NamingMode::Custom => {
            let custom = purpose.trim();
            if is_valid_branch_name(custom) {
                custom.to_owned()
            } else {
                fallback
            }
        }
    }
}

/// Today's name: the prefix, a slash, and the worktree's purpose. Unchanged, so a setting
/// that does not name a mode keeps every branch it made before.
fn static_name(prefix: &str, purpose: &str) -> String {
    format!("{prefix}/{purpose}")
}

/// `<prefix>/<slug>`, cut to the bound and suffixed `-2`, `-3` ... until it is not one of
/// `existing`. `None` when the seed or the room under the bound gives no usable name.
fn semantic_name(
    prefix: &str,
    seed: Option<&str>,
    purpose: &str,
    existing: &[String],
) -> Option<String> {
    let prefix = prefix.trim_end_matches('/');
    let head = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}/")
    };
    let room = MAX_BRANCH_CHARS.checked_sub(head.chars().count())?;
    let seed = seed
        .and_then(|seed| slug(seed, room))
        .or_else(|| slug(purpose, room))?;
    Some(unique(&head, &seed, existing))
}

/// The first name, else the first free `-N` suffix. A name is taken when it is a branch,
/// is a folder of one (`a/b` blocks `a`), or lies under one (`a` blocks `a/b`): Git
/// refuses each of those, and refs are compared without case so a case-insensitive disk
/// cannot silently reuse one.
fn unique(head: &str, slug: &str, existing: &[String]) -> String {
    let candidate = format!("{head}{slug}");
    if !is_taken(&candidate, existing) {
        return candidate;
    }
    for suffix_number in 2..=MAX_COLLISION_TRIES {
        let suffix = format!("-{suffix_number}");
        let room = MAX_BRANCH_CHARS.saturating_sub(head.chars().count() + suffix.chars().count());
        let stem = truncate(slug, room);
        if stem.is_empty() {
            break;
        }
        let candidate = format!("{head}{stem}{suffix}");
        if !is_taken(&candidate, existing) {
            return candidate;
        }
    }
    // Every suffix is in use: hand the bare name to Git, which refuses it.
    format!("{head}{slug}")
}

fn is_taken(candidate: &str, existing: &[String]) -> bool {
    let candidate = candidate.to_lowercase();
    existing.iter().any(|name| {
        let name = name.to_lowercase();
        name == candidate
            || name
                .strip_prefix(&candidate)
                .is_some_and(|rest| rest.starts_with('/'))
            || candidate
                .strip_prefix(&name)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Lowercase ASCII letters, digits and single hyphens: a name Git accepts and every
/// shell, URL and Windows path shows unchanged. `max` counts characters.
fn slug(raw: &str, max: usize) -> Option<String> {
    let mut slug = String::new();
    let mut separator = false;
    for character in raw.chars() {
        if slug.len() >= max {
            break;
        }
        let character = fold(character);
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() && slug.len() < max {
                slug.push('-');
            }
            separator = false;
            if slug.len() < max {
                slug.push(character.to_ascii_lowercase());
            }
        } else {
            separator = true;
        }
    }
    let slug = slug.trim_end_matches('-');
    (!slug.is_empty()).then(|| slug.to_owned())
}

/// The ASCII letter a Latin accent stands for, so `Cafe` with an accent reads `cafe`
/// rather than `caf`. Other non-ASCII characters separate words.
fn fold(character: char) -> char {
    let lower = character.to_lowercase().next().unwrap_or(character);
    match lower {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => 'i',
        'ł' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' => 's',
        'ť' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    }
}

fn truncate(slug: &str, max: usize) -> String {
    let cut: String = slug.chars().take(max).collect();
    cut.trim_end_matches('-').to_owned()
}

/// Git's own rules for a branch name, as far as a name this service makes can break them.
fn is_valid_branch_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_BRANCH_CHARS
        && !name.starts_with('-')
        && name != "@"
        && !name.ends_with('.')
        && !name.ends_with('/')
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("@{")
        && !name.contains("//")
        && !name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\' | '\u{7f}'
                )
        })
        && name
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naming(mode: NamingMode) -> Naming {
        Naming {
            mode,
            prefix: "feat".to_owned(),
        }
    }

    #[test]
    fn a_slug_folds_case_accents_and_punctuation() {
        assert_eq!(
            slug("Add Login Screen!", 64).as_deref(),
            Some("add-login-screen")
        );
        assert_eq!(slug("Café  déjà-vu", 64).as_deref(), Some("cafe-deja-vu"));
        assert_eq!(
            slug("  ..fix__the//bug..  ", 64).as_deref(),
            Some("fix-the-bug")
        );
        assert_eq!(
            slug("Issue #123: crash", 64).as_deref(),
            Some("issue-123-crash")
        );
        assert_eq!(slug("日本語", 64), None);
        assert_eq!(slug("   ", 64), None);
    }

    #[test]
    fn a_slug_is_bounded_and_keeps_no_trailing_separator() {
        let long = slug(&"a".repeat(500), 10).unwrap();
        assert_eq!(long, "a".repeat(10));
        assert_eq!(long.chars().count(), 10);
        // The cut lands on a separator, which is trimmed rather than left at the end.
        assert_eq!(slug("abcdefghij kl", 11).as_deref(), Some("abcdefghij"));
        assert_eq!(slug(&"x".repeat(63), 0), None);
    }

    #[test]
    fn static_naming_is_the_prefix_and_the_purpose_unchanged() {
        assert_eq!(
            branch_name(
                &naming(NamingMode::Static),
                Some("Add login"),
                "fix-login",
                &[]
            ),
            "feat/fix-login"
        );
        let mut with_slash = naming(NamingMode::Static);
        with_slash.prefix = "team/feat".to_owned();
        assert_eq!(
            branch_name(&with_slash, None, "fix-login", &[]),
            "team/feat/fix-login"
        );
    }

    #[test]
    fn semantic_naming_uses_the_title_then_the_purpose() {
        assert_eq!(
            branch_name(
                &naming(NamingMode::Semantic),
                Some("Add sign-up screen"),
                "signup",
                &[]
            ),
            "feat/add-sign-up-screen"
        );
        assert_eq!(
            branch_name(&naming(NamingMode::Semantic), None, "wire-login", &[]),
            "feat/wire-login"
        );
        // A title with nothing usable falls through to the purpose, not to an empty name.
        assert_eq!(
            branch_name(
                &naming(NamingMode::Semantic),
                Some("!!!"),
                "wire-login",
                &[]
            ),
            "feat/wire-login"
        );
    }

    #[test]
    fn semantic_naming_is_bounded_and_a_valid_ref() {
        let long_title = "Long title word ".repeat(20);
        let name = branch_name(
            &naming(NamingMode::Semantic),
            Some(long_title.as_str()),
            "purpose",
            &[],
        );
        assert!(name.chars().count() <= MAX_BRANCH_CHARS, "{name}");
        assert!(name.starts_with("feat/"), "{name}");
        assert!(is_valid_branch_name(&name), "{name}");
        // The prefix leaves the room the slug may use.
        let mut long_prefix = naming(NamingMode::Semantic);
        long_prefix.prefix = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz".to_owned();
        let name = branch_name(&long_prefix, Some("add login"), "purpose", &[]);
        assert!(name.chars().count() <= MAX_BRANCH_CHARS, "{name}");
        assert!(is_valid_branch_name(&name), "{name}");
    }

    #[test]
    fn semantic_naming_avoids_existing_branches() {
        let existing = vec!["feat/add-login".to_owned()];
        assert_eq!(
            branch_name(
                &naming(NamingMode::Semantic),
                Some("Add login"),
                "login",
                &existing
            ),
            "feat/add-login-2"
        );
        let existing = vec![
            "feat/add-login".to_owned(),
            "feat/add-login-2".to_owned(),
            "feat/add-login-3".to_owned(),
        ];
        assert_eq!(
            branch_name(
                &naming(NamingMode::Semantic),
                Some("Add login"),
                "login",
                &existing
            ),
            "feat/add-login-4"
        );
    }

    #[test]
    fn semantic_naming_treats_git_name_folders_and_case_as_taken() {
        // A sibling branch is not a folder of the candidate.
        let existing = vec!["feat/add".to_owned()];
        assert_eq!(
            branch_name(
                &naming(NamingMode::Semantic),
                Some("Add login"),
                "login",
                &existing
            ),
            "feat/add-login"
        );
        // The same ref in another case is taken, as a case-insensitive disk would see it.
        let existing = vec!["FEAT/Add-Login".to_owned()];
        assert_eq!(
            branch_name(
                &naming(NamingMode::Semantic),
                Some("Add login"),
                "login",
                &existing
            ),
            "feat/add-login-2"
        );
        // A ref under the candidate blocks it.
        let existing = vec!["feat/add-login/next".to_owned()];
        assert_eq!(
            branch_name(
                &naming(NamingMode::Semantic),
                Some("Add login"),
                "login",
                &existing
            ),
            "feat/add-login-2"
        );
    }

    #[test]
    fn semantic_naming_falls_back_to_static_when_no_name_is_usable() {
        // A prefix that leaves no room for a slug cannot make a semantic name.
        let mut cramped = naming(NamingMode::Semantic);
        cramped.prefix = "x".repeat(MAX_BRANCH_CHARS);
        assert_eq!(
            branch_name(&cramped, Some("Add login"), "fix-login", &[]),
            format!("{}/fix-login", "x".repeat(MAX_BRANCH_CHARS))
        );
    }

    #[test]
    fn custom_naming_is_the_purpose_or_the_static_fallback() {
        assert_eq!(
            branch_name(
                &naming(NamingMode::Custom),
                Some("Add login"),
                "fix-login",
                &[]
            ),
            "fix-login"
        );
        // Not a valid branch name: the static name is used instead of inventing one.
        assert_eq!(
            branch_name(&naming(NamingMode::Custom), None, "fix login", &[]),
            "feat/fix login"
        );
    }

    #[test]
    fn the_same_inputs_always_give_the_same_name() {
        let one = branch_name(
            &naming(NamingMode::Semantic),
            Some("Fix the flaky test"),
            "flaky",
            &["feat/fix-the-flaky-test".to_owned()],
        );
        let two = branch_name(
            &naming(NamingMode::Semantic),
            Some("Fix the flaky test"),
            "flaky",
            &["feat/fix-the-flaky-test".to_owned()],
        );
        assert_eq!(one, two);
        assert_eq!(one, "feat/fix-the-flaky-test-2");
    }
}
