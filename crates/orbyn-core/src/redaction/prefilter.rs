//! The one-pass filter that says which detectors can match a text, so the ones that cannot skip
//! their own pass. One apply used to run every pattern over the text: up to about 60 separate
//! passes, most of them finding nothing. The set here is asked once per window, and only the
//! detectors it names run, so ordinary text costs one pass instead of sixty.
//!
//! It is a filter, never a decision. `RegexSet::matches` answers for each pattern exactly what
//! `Regex::is_match` answers about the same text, so a detector the set leaves out could not have
//! found anything in it: the spans are the ones the unfiltered path finds. Two things are left out
//! of the set, and both keep their own pass on every text: a pattern whose compiled program is too
//! big to sit in a set (the private-key format's 8 KiB bounded repeat is one), and patterns whose
//! set could not be built at all. So a large pattern costs its own pass, never every text's.

use regex::{RegexSet, RegexSetBuilder};

/// The program one set may take. A set sits beside the detectors' own compiled patterns in a
/// cached redactor, so keep it small: a pattern that needs more keeps its own pass instead.
const SET_BYTES: usize = 1 << 20;
/// The lazily built DFA of a set, bounded the way `custom` bounds a person's pattern.
const SET_DFA_BYTES: usize = 1 << 20;

/// The detectors' patterns, split by what each reads: the text itself, or only the value of a
/// `NAME=value` assignment.
#[derive(Default)]
pub(crate) struct Prefilter {
    /// The patterns that read the whole text.
    anywhere: Compiled,
    /// The patterns that read only an assignment's value.
    values: Compiled,
}

impl Prefilter {
    /// The set for these detectors, each as (its index, its pattern, whether it reads only a
    /// value). A detector's pattern is compiled again in the set; it is the same pattern text, so
    /// the set's answer is the answer its own pass would give.
    pub(crate) fn new<'a>(detectors: impl IntoIterator<Item = (usize, &'a str, bool)>) -> Self {
        let split = Split::of(detectors);
        Self {
            anywhere: Compiled::of(&split.anywhere_at, &split.anywhere_sources),
            values: Compiled::of(&split.values_at, &split.values_sources),
        }
    }

    /// Whether a set was compiled: false means every detector runs its own pass. A test uses this
    /// to prove the filter is live rather than silently falling back.
    #[cfg(test)]
    pub(crate) fn is_compiled(&self) -> bool {
        self.anywhere.set.is_some() || self.values.set.is_some()
    }

    /// How many patterns the text set holds, and which ones it could not hold. A test uses these
    /// to prove the filter is live and that only a pattern too big for a set is left out.
    #[cfg(test)]
    pub(crate) fn held_and_apart(&self) -> (usize, Vec<usize>) {
        (self.anywhere.set_at.len(), self.anywhere.apart.clone())
    }

    /// The same detectors with no set compiled, the shape a set that cannot compile falls back to.
    #[cfg(test)]
    pub(crate) fn uncompiled(&self) -> Self {
        Self {
            anywhere: Compiled::all(self.anywhere.every()),
            values: Compiled::all(self.values.every()),
        }
    }

    /// Whether any pattern reads only an assignment's value, so the scan knows to read them.
    pub(crate) fn reads_values(&self) -> bool {
        self.values.any()
    }

    /// The detectors that can match `hay` anywhere, in detector order: every one of them when the
    /// set is not compiled, and the always-run ones plus those the set names otherwise.
    pub(crate) fn anywhere(&self, hay: &str) -> Vec<usize> {
        self.anywhere.matching(hay)
    }

    /// The detectors that can match inside one assignment's `value`, in detector order.
    pub(crate) fn values(&self, value: &str) -> Vec<usize> {
        self.values.matching(value)
    }
}

/// One set and the detectors around it: the ones it holds, and the ones that keep their own pass.
#[derive(Default)]
struct Compiled {
    /// The set, when one was compiled.
    set: Option<RegexSet>,
    /// Detector indices of the set's patterns, in the set's own order.
    set_at: Vec<usize>,
    /// Detector indices the set could not hold: they run their own pass on every text.
    apart: Vec<usize>,
}

impl Compiled {
    /// The set for `sources`, whose detector indices are `at`. A pattern too big to join it is
    /// left apart, and so is every pattern when the rest still will not fit together.
    fn of(at: &[usize], sources: &[&str]) -> Self {
        if sources.is_empty() {
            return Self::default();
        }
        if let Ok(set) = build(sources) {
            return Self {
                set: Some(set),
                set_at: at.to_vec(),
                apart: Vec::new(),
            };
        }
        let mut joined_at = Vec::new();
        let mut joined = Vec::new();
        let mut apart = Vec::new();
        for (slot, source) in sources.iter().enumerate() {
            if build(&[*source]).is_ok() {
                joined_at.push(at[slot]);
                joined.push(*source);
            } else {
                apart.push(at[slot]);
            }
        }
        if joined.is_empty() {
            return Self::all(at.to_vec());
        }
        match build(&joined) {
            Ok(set) => Self {
                set: Some(set),
                set_at: joined_at,
                apart,
            },
            Err(_) => Self::all(at.to_vec()),
        }
    }

    /// Every pattern apart: no set, so every detector runs its own pass.
    fn all(at: Vec<usize>) -> Self {
        Self {
            set: None,
            set_at: Vec::new(),
            apart: at,
        }
    }

    /// Every detector index this holds or leaves apart.
    #[cfg(test)]
    fn every(&self) -> Vec<usize> {
        let mut every = self.set_at.clone();
        every.extend_from_slice(&self.apart);
        every.sort_unstable();
        every
    }

    fn any(&self) -> bool {
        self.set.is_some() || !self.apart.is_empty()
    }

    fn matching(&self, hay: &str) -> Vec<usize> {
        let mut found = self.apart.clone();
        if let Some(set) = &self.set {
            found.extend(set.matches(hay).iter().map(|at| self.set_at[at]));
        }
        found.sort_unstable();
        found
    }
}

/// The detectors' patterns and indices, before either set is compiled.
struct Split<'a> {
    anywhere_at: Vec<usize>,
    anywhere_sources: Vec<&'a str>,
    values_at: Vec<usize>,
    values_sources: Vec<&'a str>,
}

impl<'a> Split<'a> {
    fn of(detectors: impl IntoIterator<Item = (usize, &'a str, bool)>) -> Self {
        let mut split = Self {
            anywhere_at: Vec::new(),
            anywhere_sources: Vec::new(),
            values_at: Vec::new(),
            values_sources: Vec::new(),
        };
        for (index, source, reads_values) in detectors {
            if reads_values {
                split.values_at.push(index);
                split.values_sources.push(source);
            } else {
                split.anywhere_at.push(index);
                split.anywhere_sources.push(source);
            }
        }
        split
    }
}

fn build(sources: &[&str]) -> Result<RegexSet, regex::Error> {
    RegexSetBuilder::new(sources.iter().copied())
        .size_limit(SET_BYTES)
        .dfa_size_limit(SET_DFA_BYTES)
        .build()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use orbyn_contracts::{PatternKind, PatternScope};

    use super::super::custom::MAX_APPLY_BYTES;
    use super::super::{Custom, Redactor, compile};

    /// A pattern the way `Redactor::with` receives one, built from parts where it stands for a
    /// secret: no literal in this file is a secret.
    fn custom(name: &'static str, source: &str, applies_to: PatternScope) -> Custom<'static> {
        Custom {
            name,
            regex: compile(PatternKind::Regex, source).expect("a test pattern compiles"),
            applies_to,
        }
    }

    fn key() -> String {
        format!("{}A{}", "AKI", "Q".repeat(16))
    }

    fn github() -> String {
        format!("{}_{}", "ghp", "b2".repeat(20))
    }

    /// The filtered path and the unfiltered one must agree on every masking call, for this text.
    fn same_output(redactor: &Redactor, text: &str) {
        let slow = redactor.unfiltered();
        assert_eq!(
            redactor.spans(text),
            slow.spans(text),
            "spans differ for {} bytes of text",
            text.len()
        );
        assert_eq!(
            redactor.labelled(text),
            slow.labelled(text),
            "the mask differs for {} bytes of text",
            text.len()
        );
    }

    #[test]
    fn the_prefilter_is_live_and_only_a_too_big_pattern_keeps_its_own_pass() {
        let redactor = Redactor::locked();
        assert!(
            redactor.prefilter().is_compiled(),
            "the set fell back, so nothing was filtered"
        );
        let (held, apart) = redactor.prefilter().held_and_apart();
        assert!(held > 0, "the set holds no pattern");
        assert!(
            !apart.is_empty(),
            "the private-key pattern should not fit a 1 MiB set"
        );
        assert!(
            held > apart.len(),
            "most recognised formats should fit the set"
        );
        // Ordinary text names only the patterns that keep their own pass.
        assert_eq!(
            redactor
                .prefilter()
                .anywhere("ordinary prose, a path and 12 numbers"),
            {
                let mut apart = apart.clone();
                apart.sort_unstable();
                apart
            },
        );
        // A value the set can recognise adds a detector the set holds.
        assert!(
            redactor.prefilter().anywhere(&key()).len() > apart.len(),
            "the set named no detector for a value it must hide"
        );
    }

    #[test]
    fn the_filtered_and_unfiltered_paths_agree_on_adversarial_text() {
        let redactor = Redactor::locked();
        let examples = [
            // Nothing to redact, and nothing at all.
            String::new(),
            " \n\t ".to_owned(),
            "The quick brown fox jumps over the lazy dog, twice.".repeat(40),
            // Values that partly match: below the threshold, so nothing may be hidden.
            format!("{}_short", "sk"),
            format!("{}A{}", "AKI", "Q".repeat(15)),
            format!("{}_", "ghp"),
            // Two secrets touching, with no separator and then a line apart.
            format!("{}{}", github(), key()),
            format!("{}\n{}", github(), key()),
            // A value a built-in and a person's pattern both read.
            format!("the key {} is here", key()),
            // Formats next to each other, one inside an assignment.
            format!("HOST={} and id={}", key(), github()),
            // Case-insensitive formats, and a header that hides the rest.
            format!("{} {}", "bearer", "a".repeat(24)),
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEvQ\n".to_owned(),
            // An empty match, which hides nothing and may not litter the text.
            "axxbxxc".to_owned(),
        ];
        for text in &examples {
            same_output(redactor, text);
        }
        // A person's own pattern, a broad one, and many patterns at once.
        let many: Vec<Custom<'_>> = (0..40)
            .map(|n| {
                let name = Box::leak(format!("pattern {n}").into_boxed_str());
                custom(name, &format!("canary{n:02}"), PatternScope::Anywhere)
            })
            .chain(Some(custom("acme", "acme_secret", PatternScope::Anywhere)))
            .collect();
        let mixed = Redactor::with(&BTreeSet::new(), many);
        for text in [
            format!("one acme_secret and canary07 twice {}", key()),
            "nothing here but prose".to_owned(),
            format!("{}{}", "canary1", "canary2"),
        ] {
            same_output(&mixed, &text);
        }
        // A pattern that can match empty is still read the way it was.
        let empty = Redactor::only(vec![custom("empty", "x*", PatternScope::Anywhere)]);
        for text in ["abc", "axxb", "", "xxxx"] {
            same_output(&empty, text);
        }
    }

    #[test]
    fn a_span_two_detectors_find_keeps_the_first_detectors_label() {
        // The built-in format comes before the person's pattern, so its label wins, as it did
        // before the filter: the scan's own order must not decide it.
        let both = Redactor::with(
            &BTreeSet::new(),
            vec![custom(
                "Acme key",
                &regex::escape(&key()),
                PatternScope::Anywhere,
            )],
        );
        let text = format!("id {} end", key());
        let shown = both.labelled(&text);
        assert!(
            shown.contains("[hidden: AWS access key]"),
            "the later detector named the span: {shown}"
        );
        same_output(&both, &text);
    }

    #[test]
    fn an_environment_pattern_still_reads_only_assignments() {
        let env = Redactor::only(vec![custom(
            "Corp id",
            "corp[0-9]+",
            PatternScope::EnvironmentValues,
        )]);
        let text = "HOST=corp42 and ping corp43\nTOKEN=\"a corp44 b\"\nplain corp45";
        assert_eq!(
            env.labelled(text),
            "HOST=[hidden: Corp id] and ping corp43\nTOKEN=\"a [hidden: Corp id] b\"\nplain corp45"
        );
        same_output(&env, text);
        // The same pattern read as text hides every one of them, so the two scopes stay distinct.
        let anywhere = Redactor::only(vec![custom(
            "Corp id",
            "corp[0-9]+",
            PatternScope::Anywhere,
        )]);
        same_output(&anywhere, text);
        assert!(anywhere.labelled(text).matches("[hidden:").count() > 1);
    }

    #[test]
    fn a_text_longer_than_one_window_masks_the_same_at_its_edges() {
        let secret = key();
        // A secret at the start, and one past the first window's end. Each sits on its own line:
        // a key run into a word character has no `\b` and is not a match.
        let mut text = String::with_capacity(MAX_APPLY_BYTES + 4096);
        text.push_str(&format!("\n{secret}\n"));
        text.push_str(&"filler line\n".repeat(MAX_APPLY_BYTES / 12));
        text.push_str(&format!("\n{secret}\n"));
        text.push_str("tail\n");
        assert!(text.len() > MAX_APPLY_BYTES, "the text fits one window");
        let redactor = Redactor::locked();
        let slow = redactor.unfiltered();
        assert_eq!(
            redactor.spans(&text),
            slow.spans(&text),
            "the windows found different spans"
        );
        assert!(
            redactor.labelled(&text).matches("[hidden:").count() >= 2,
            "a secret in a long text was not hidden"
        );
    }
}
