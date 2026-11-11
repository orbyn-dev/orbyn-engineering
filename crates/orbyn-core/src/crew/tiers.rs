//! Which tier a role needs, and which of an engine's own models fills a tier.
//!
//! The plan's table (docs/crew.md, "Models and effort per role") names the tiers by model
//! family: Opus, Sonnet and Haiku on Claude; Sol, Terra and Luna on Codex. A name is only a
//! default, matched against the models this machine lists for the engine. An engine with no
//! mapping (OpenCode and the other ACP engines, until Settings maps them) is a builder only.

use std::collections::BTreeMap;

use orbyn_contracts::{BriefKind, ModelInfo, RouteRole, RouteTier};

/// The words that mark a model as one tier's, matched as whole words in its id or label,
/// ignoring case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TierWords {
    pub lead: Option<String>,
    pub worker: Option<String>,
    pub scout: Option<String>,
}

impl TierWords {
    fn of(lead: &str, worker: &str, scout: &str) -> Self {
        Self {
            lead: Some(lead.into()),
            worker: Some(worker.into()),
            scout: Some(scout.into()),
        }
    }

    fn word(&self, tier: RouteTier) -> Option<&str> {
        match tier {
            RouteTier::Lead => self.lead.as_deref(),
            RouteTier::Worker => self.worker.as_deref(),
            RouteTier::Scout => self.scout.as_deref(),
        }
    }
}

/// The router's defaults. A later slice may fill it from Settings; the router reads only this.
#[derive(Clone, Debug)]
pub struct RouterConfig {
    /// By engine family.
    pub tiers: BTreeMap<String, TierWords>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            tiers: BTreeMap::from([
                (
                    "claude".to_owned(),
                    TierWords::of("opus", "sonnet", "haiku"),
                ),
                ("codex".to_owned(), TierWords::of("sol", "terra", "luna")),
            ]),
        }
    }
}

impl RouterConfig {
    fn words(&self, family: &str) -> TierWords {
        self.tiers.get(family).cloned().unwrap_or_default()
    }
}

/// The role a brief gets when nobody names one. A review brief is a code review; research is
/// a scout's search; everything else is built.
pub fn role_for_kind(kind: BriefKind) -> RouteRole {
    match kind {
        BriefKind::Review => RouteRole::CodeReview,
        BriefKind::Research => RouteRole::Scout,
        BriefKind::Plan
        | BriefKind::Implement
        | BriefKind::Test
        | BriefKind::Ui
        | BriefKind::Docs => RouteRole::Builder,
    }
}

/// Reviewers, the finding check and the second key are lead tier (the harness routes
/// `reviewer`, `security` and `lead` to the lead tier); builders are worker tier.
pub fn role_tier(role: RouteRole) -> RouteTier {
    match role {
        RouteRole::Builder => RouteTier::Worker,
        RouteRole::Scout => RouteTier::Scout,
        RouteRole::CodeReview
        | RouteRole::SecurityReview
        | RouteRole::FindingCheck
        | RouteRole::SecondKey
        | RouteRole::Review => RouteTier::Lead,
    }
}

/// A role that checks someone else's work, for the other-vendor rule.
pub fn reviews(role: RouteRole) -> bool {
    !matches!(role, RouteRole::Builder | RouteRole::Scout)
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// True when `word` (one or more hyphen- or space-separated words) appears as whole words
/// in `text`, so `sol` matches `sol-1` and `Sol` but not `console` or `solid`.
fn has_words(text: &str, word: &str) -> bool {
    let (text, word) = (words(text), words(word));
    !word.is_empty() && text.windows(word.len()).any(|run| run == word.as_slice())
}

fn mentions(model: &ModelInfo, word: &str) -> bool {
    has_words(&model.id, word) || has_words(&model.label, word)
}

/// The tier a model fills on `family`, when the family has a mapping that names it.
pub fn tier_of_model(config: &RouterConfig, family: &str, model: &ModelInfo) -> Option<RouteTier> {
    let words = config.words(family);
    [RouteTier::Lead, RouteTier::Worker, RouteTier::Scout]
        .into_iter()
        .find(|tier| words.word(*tier).is_some_and(|word| mentions(model, word)))
}

/// Why no model of the engine fills what was asked.
#[derive(Debug, PartialEq, Eq)]
pub enum Unfit {
    /// The engine lists no model at all.
    NoModels,
    /// The tier has no model here: not mapped for this engine, or none listed.
    Tier(RouteTier),
    /// A needed input (an image) is offered by no candidate model.
    Input(&'static str),
    /// A model that was asked for by name is not among the listed ones.
    Named,
}

fn preferred<'a>(models: impl Iterator<Item = &'a ModelInfo>) -> Option<&'a ModelInfo> {
    let all: Vec<&ModelInfo> = models.collect();
    all.iter().find(|m| m.is_default).or(all.first()).copied()
}

fn takes_image(model: &ModelInfo) -> bool {
    model.input_modalities.iter().any(|kind| kind == "image")
}

/// The model that fills `tier` on this engine: the engine's default among the models that
/// match the tier's word, else the first listed. An unmapped engine fills only the worker
/// tier, with its default model.
pub fn model_for_tier<'a>(
    config: &RouterConfig,
    family: &str,
    models: &'a [ModelInfo],
    tier: RouteTier,
    image: bool,
) -> Result<&'a ModelInfo, Unfit> {
    if models.is_empty() {
        return Err(Unfit::NoModels);
    }
    let words = config.words(family);
    let pool: Vec<&ModelInfo> = match words.word(tier) {
        Some(word) => models.iter().filter(|m| mentions(m, word)).collect(),
        None if tier == RouteTier::Worker => models.iter().collect(),
        None => return Err(Unfit::Tier(tier)),
    };
    if pool.is_empty() {
        return Err(Unfit::Tier(tier));
    }
    if image {
        return preferred(pool.into_iter().filter(|m| takes_image(m))).ok_or(Unfit::Input("image"));
    }
    preferred(pool.into_iter()).ok_or(Unfit::Tier(tier))
}

/// A model the lead or person named: the exact id, else the one whose id or label mentions
/// the name as whole words (`sonnet`, `sol`), ignoring case.
pub fn model_named<'a>(
    models: &'a [ModelInfo],
    name: &str,
    image: bool,
) -> Result<&'a ModelInfo, Unfit> {
    let name = name.to_lowercase();
    let fits = |m: &&ModelInfo| !image || takes_image(m);
    let exact = models
        .iter()
        .filter(fits)
        .find(|m| m.id.to_lowercase() == name);
    let found = exact.or_else(|| models.iter().filter(fits).find(|m| mentions(m, &name)));
    match (found, image) {
        (Some(model), _) => Ok(model),
        (None, true)
            if models
                .iter()
                .any(|m| mentions(m, &name) || m.id.to_lowercase() == name) =>
        {
            Err(Unfit::Input("image"))
        }
        (None, _) => Err(Unfit::Named),
    }
}
