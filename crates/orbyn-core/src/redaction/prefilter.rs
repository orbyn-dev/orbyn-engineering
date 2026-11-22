//! The one-pass filter that says which detectors can match a text, so the ones that cannot skip
//! their own pass. One apply used to run every pattern over the text: up to about 60 separate
//! passes, most of them finding nothing. The set here is asked once per window, and only the
//! detectors it names run, so ordinary text costs one pass instead of sixty.
//!
//! It is a filter, never a decision. `RegexSet::matches` answers for each pattern exactly what
//! `Regex::is_match` answers about the same text, so a detector the set leaves out could not have
//! found anything in it: the spans are the ones the unfiltered path finds. A set that cannot be
//! compiled under the size limit is dropped, and every detector runs its own pass again.

use regex::{RegexSet, RegexSetBuilder};

/// The program one set may take. A set sits beside the detectors' own compiled patterns in a
/// cached redactor, so keep it small: a set that needs more is dropped, not made bigger.
const SET_BYTES: usize = 1 << 20;
/// The lazily built DFA of a set, bounded the way `custom` bounds a person's pattern.
const SET_DFA_BYTES: usize = 1 << 20;

/// The detectors' patterns, split by what each reads: the text itself, or only the value of a
/// `NAME=value` assignment. Each keeps its detector index, so a match names the detector again.
pub(crate) struct Prefilter {
    /// The detectors whose pattern reads the whole text, in detector order.
    anywhere_at: Vec<usize>,
    /// Which of them can match a given text; `None` when the set is not compiled.
    anywhere: Option<RegexSet>,
    /// The detectors whose pattern reads only an assignment's value, in detector order.
    values_at: Vec<usize>,
    /// Which of them can match a given value; `None` when the set is not compiled.
    values: Option<RegexSet>,
}

impl Default for Prefilter {
    fn default() -> Self {
        Self {
            anywhere_at: Vec::new(),
            anywhere: None,
            values_at: Vec::new(),
            values: None,
        }
    }
}

impl Prefilter {
    /// The set for these detectors, each as (its index, its pattern, whether it reads only a
    /// value). A detector's pattern is compiled again in the set; it is the same pattern text, so
    /// the set's answer is the answer its own pass would give.
    pub(crate) fn new<'a>(detectors: impl IntoIterator<Item = (usize, &'a str, bool)>) -> Self {
        let split = Split::of(detectors);
        Self {
            anywhere: set(&split.anywhere_sources),
            anywhere_at: split.anywhere_at,
            values: set(&split.values_sources),
            values_at: split.values_at,
        }
    }

    /// Whether a set was compiled: false means every detector runs its own pass. A test uses this
    /// to prove the filter is live rather than silently falling back.
    #[cfg(test)]
    pub(crate) fn is_compiled(&self) -> bool {
        (self.anywhere_at.is_empty() || self.anywhere.is_some())
            && (self.values_at.is_empty() || self.values.is_some())
    }

    /// The same detectors with no set compiled, the shape a set that cannot compile falls back to.
    #[cfg(test)]
    pub(crate) fn uncompiled(&self) -> Self {
        Self {
            anywhere_at: self.anywhere_at.clone(),
            anywhere: None,
            values_at: self.values_at.clone(),
            values: None,
        }
    }

    /// Whether any pattern reads only an assignment's value, so the scan knows to read them.
    pub(crate) fn reads_values(&self) -> bool {
        !self.values_at.is_empty()
    }

    /// The detectors that can match `hay` anywhere, in detector order: every one of them when the
    /// set is not compiled, none when `hay` holds nothing a pattern matches.
    pub(crate) fn anywhere(&self, hay: &str) -> Vec<usize> {
        match &self.anywhere {
            Some(set) => set
                .matches(hay)
                .iter()
                .map(|at| self.anywhere_at[at])
                .collect(),
            None => self.anywhere_at.clone(),
        }
    }

    /// The detectors that can match inside one assignment's `value`, in detector order.
    pub(crate) fn values(&self, value: &str) -> Vec<usize> {
        match &self.values {
            Some(set) => set
                .matches(value)
                .iter()
                .map(|at| self.values_at[at])
                .collect(),
            None => self.values_at.clone(),
        }
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

fn set(sources: &[&str]) -> Option<RegexSet> {
    if sources.is_empty() {
        return None;
    }
    RegexSetBuilder::new(sources.iter().copied())
        .size_limit(SET_BYTES)
        .dfa_size_limit(SET_DFA_BYTES)
        .build()
        .ok()
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
    fn the_prefilter_is_live_and_names_fewer_detectors_than_it_filters() {
        let redactor = Redactor::locked();
        assert!(
            redactor.prefilter().is_compiled(),
            "the set fell back, so nothing was filtered"
        );
        assert!(
            redactor
                .prefilter()
                .anywhere("ordinary prose, a path and 12 numbers")
                .is_empty(),
            "the set named a detector for text that hides nothing"
        );
        assert!(
            !redactor.prefilter().anywhere(&key()).is_empty(),
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
            format!("the key {key} is here"),
            // Formats next to each other, one inside an assignment.
            format!("HOST={} and id={}", key(), github()),
            // Case-insensitive formats, and a header that hides the rest.
            format!("{} {}", "bearer", "a".repeat(24)),
            format!("-----BEGIN RSA PRIVATE KEY-----\nMIIEvQ\n"),
            // An empty match, which hides nothing and may not litter the text.
            "axxbxxc".to_owned(),
        ];
        for text in &examples {
            same_output(&redactor, text);
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
        // A secret at the start, one straddling the first window's end, and one past it.
        let mut text = String::with_capacity(MAX_APPLY_BYTES + 4096);
        text.push_str(&secret);
        text.push_str(&"filler line\n".repeat(MAX_APPLY_BYTES / 12));
        text.push_str(&secret);
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
