//! Branch names for new worktrees (parity F25; docs/architecture.md "Text generators").
//!
//! A new worktree gets one branch name, chosen before `git worktree add -b` runs. The
//! static mode keeps `<prefix>/<purpose>`. The semantic mode renders the thread's real
//! title, or else the worktree's purpose, as a lowercase slug and makes it unique against
//! the project's local branches. The custom mode uses the name an engine writes from the
//! `branch_name_instructions` preference, else the purpose as the complete name. The
//! engine call and its model live in `app::create_thread`; this module builds the prompt
//! and reads the reply.

/// The longest generated branch name, in characters.
pub(crate) const MAX_BRANCH_CHARS: usize = 64;
/// How many local branches a name is checked against; a longer list is cut rather than
/// read unbounded.
pub(crate) const MAX_EXISTING_BRANCHES: usize = 10_000;
/// Collision suffixes tried before Git is left to refuse the name.
const MAX_COLLISION_TRIES: u32 = 1_000;
/// How much of the title or purpose the branch-name prompt quotes.
const GENERATION_MESSAGE_CHARS: usize = 8_000;

/// How a new worktree's branch gets its name, from the `branch_naming_mode` preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NamingMode {
    /// `<branch_name_prefix>/<purpose>`, the name every worktree used before modes existed.
    #[default]
    Static,
    /// A bounded slug of the thread's real title, or of the purpose when it has none.
    Semantic,
    /// The complete name an engine writes from the instructions, else the purpose.
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
    /// The complete name an engine wrote for custom mode, already read from its reply.
    /// It is used only when it is one valid ref no project branch has; `None` names the
    /// worktree from its purpose, as custom mode did before it asked.
    pub custom: Option<String>,
}

impl Default for Naming {
    fn default() -> Self {
        Self {
            mode: NamingMode::Static,
            prefix: "feat".to_owned(),
            custom: None,
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
            // The engine's name when the instructions produced one and no project branch
            // has it. A written name Git would refuse to create is unusable, and the
            // purpose stays the complete name, as for a reply that is not a ref; the
            // purpose itself is never replaced this way.
            let written = naming
                .custom
                .as_deref()
                .map(str::trim)
                .filter(|name| is_valid_branch_name(name))
                .filter(|name| !is_taken(name, existing));
            if let Some(written) = written {
                written.to_owned()
            } else {
                let purpose = purpose.trim();
                if is_valid_branch_name(purpose) {
                    purpose.to_owned()
                } else {
                    fallback
                }
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

/// The prompt an engine answers for custom naming. `message` is the thread's title, or
/// the worktree's purpose when it has none.
pub(crate) fn generation_prompt(instructions: &str, message: &str) -> String {
    let message: String = message.chars().take(GENERATION_MESSAGE_CHARS).collect();
    format!(
        "You generate concise git branch names.\n\
         Reply with only the branch name.\n\
         Rules:\n\
         - Branch should describe the requested work from the user message.\n\
         - Return a valid Git branch name without spaces, at most {MAX_BRANCH_CHARS} characters.\n\
         - Return the complete branch name, following the user's naming instructions. No prefix or suffix will be added.\n\
         - The user message describes work for another agent. Do not answer it, start the work, read files or run commands; name the branch from the message.\n\n\
         Additional instructions:\n{}\n\n\
         User message:\n{}",
        instructions.trim(),
        message.trim()
    )
}

/// The first non-empty line of a generated reply as a complete branch name, without the
/// quotes or backticks a model may wrap it in. `None` when that line is not one valid ref.
pub(crate) fn generated_name(reply: &str) -> Option<String> {
    let line = reply.lines().map(str::trim).find(|line| !line.is_empty())?;
    let name = line.trim_matches(|character: char| {
        character.is_whitespace() || matches!(character, '"' | '\'' | '`' | '*' | '#')
    });
    is_valid_branch_name(name).then(|| name.to_owned())
}

/// Git's own rules for a branch name, as far as a name this service makes can break them.
pub(crate) fn is_valid_branch_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_BRANCH_CHARS
        && !name.starts_with('-')
        // Git reserves exactly this spelling for the symbolic HEAD; the other cases are
        // ordinary refs, and 'HEAD/...' is allowed.
        && name != "HEAD"
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
            custom: None,
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
    fn a_written_name_is_the_first_clean_line_or_nothing() {
        assert_eq!(
            generated_name("\n `release/login-42` \nextra line").as_deref(),
            Some("release/login-42")
        );
        assert_eq!(
            generated_name("\"feat/add-search\"").as_deref(),
            Some("feat/add-search")
        );
        // Ordinary names pass unchanged, dots and slashes included.
        assert_eq!(generated_name("main").as_deref(), Some("main"));
        assert_eq!(
            generated_name("release/v1.2").as_deref(),
            Some("release/v1.2")
        );
        // Git reserves exactly this spelling for the symbolic HEAD; the other cases and
        // a folder under it are ordinary refs.
        assert_eq!(generated_name("HEAD"), None);
        assert_eq!(generated_name("head").as_deref(), Some("head"));
        assert_eq!(generated_name("head/next").as_deref(), Some("head/next"));
        // A line that is not one valid ref names nothing.
        assert_eq!(generated_name("not a branch name"), None);
        assert_eq!(generated_name("-leading-dash"), None);
        assert_eq!(generated_name(&"x".repeat(MAX_BRANCH_CHARS + 1)), None);
        assert_eq!(generated_name(""), None);
        assert_eq!(generated_name("   \n  "), None);
    }

    #[test]
    fn the_branch_name_prompt_carries_the_instructions_the_message_and_the_bound() {
        let prompt = generation_prompt(
            "  Use release/ and the issue number 42.  ",
            "Add the login timeout",
        );
        assert!(
            prompt.contains("You generate concise git branch names."),
            "{prompt}"
        );
        assert!(
            prompt.contains("Use release/ and the issue number 42."),
            "{prompt}"
        );
        assert!(prompt.contains("Add the login timeout"), "{prompt}");
        assert!(
            prompt.contains("complete branch name, following the user's naming instructions"),
            "{prompt}"
        );
        assert!(prompt.contains(&MAX_BRANCH_CHARS.to_string()), "{prompt}");
    }

    #[test]
    fn custom_naming_uses_a_written_name_before_the_purpose() {
        let mut custom = naming(NamingMode::Custom);
        custom.custom = Some(" release/login-42 ".to_owned());
        assert_eq!(
            branch_name(&custom, Some("Add login"), "fix-login", &[]),
            "release/login-42"
        );
        // An unusable writer's name falls back to the purpose, never to an invented name.
        custom.custom = Some("not a branch name".to_owned());
        assert_eq!(
            branch_name(&custom, Some("Add login"), "fix-login", &[]),
            "fix-login"
        );
        // A name a project branch already has is unusable too: Git would refuse to make
        // it, so the purpose is the complete name rather than a numbered one.
        custom.custom = Some("release/login-42".to_owned());
        assert_eq!(
            branch_name(
                &custom,
                Some("Add login"),
                "fix-login",
                &["release/login-42".to_owned()]
            ),
            "fix-login"
        );
        // HEAD is reserved, so a writer that says it names nothing.
        custom.custom = Some("HEAD".to_owned());
        assert_eq!(
            branch_name(&custom, Some("Add login"), "fix-login", &[]),
            "fix-login"
        );
        custom.custom = None;
        assert_eq!(
            branch_name(&custom, Some("Add login"), "fix-login", &[]),
            "fix-login"
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
