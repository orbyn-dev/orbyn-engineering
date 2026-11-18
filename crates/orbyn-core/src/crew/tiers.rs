//! Which tier a role needs, and which of an engine's own models fills a tier.
//!
//! The plan's table (docs/crew.md, "Models and effort per role") names the tiers by model
//! family: Opus, Sonnet and Haiku on Claude; Sol, Terra and Luna on Codex. A name is only a
//! default, matched against the models this machine lists for the engine. An engine with no
//! mapping (OpenCode and the other ACP engines, until Settings maps them) is a builder only.

use std::collections::BTreeMap;

use orbyn_contracts::{BriefKind, EngineTier, ModelInfo, RouteRole, RouteTier};

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
    /// Explicit exact ACP engine id assignments. The model pin is resolved by
    /// core when the person saves, never guessed from its name.
    pub acp_tiers: BTreeMap<String, AcpTierAssignment>,
    /// Which provider a worker is routed to when the brief names no engine. A later Settings choice fills it; nothing sets it yet.
    pub worker_provider: WorkerProvider,
}

/// Where the router looks for a worker's engine when nothing is pinned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkerProvider {
    /// The lead's own provider first; another one only when that one is refused or not available. Pass rate and headroom
    /// choose only inside the provider.
    #[default]
    SameAsLead,
    /// Every allowed provider competes on score.
    Any,
}

pub use orbyn_contracts::CrewAcpTierAssignment as AcpTierAssignment;

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            worker_provider: WorkerProvider::default(),
            acp_tiers: BTreeMap::new(),
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

/// ACP assignments add only the explicitly assigned tier to the engine's
/// existing builder admission. They do not promote siblings or other models.
pub(super) fn acp_model<'a>(
    config: &RouterConfig,
    view: &super::view::EngineView,
    models: &'a [ModelInfo],
    tier: RouteTier,
    named: Option<&str>,
    image: bool,
) -> Result<(&'a ModelInfo, Option<RouteTier>), Unfit> {
    debug_assert_eq!(view.tier, EngineTier::Acp);
    let default = explicit_default(models);
    let model = match named {
        Some(id) => models.iter().find(|m| m.id == id).ok_or(Unfit::Named)?,
        None if tier == RouteTier::Worker => preferred(models.iter()).ok_or(Unfit::NoModels)?,
        None => default.ok_or(Unfit::Tier(tier))?,
    };
    if image && !takes_image(model) {
        return Err(Unfit::Input("image"));
    }
    let assigned = config
        .acp_tiers
        .get(view.id.as_str())
        .filter(|a| default.is_some_and(|d| a.model == d.id) && a.model == model.id);
    if tier != RouteTier::Worker && !assigned.is_some_and(|a| a.tier == tier) {
        return Err(Unfit::Tier(tier));
    }
    // `None` is an engine the person has not assigned a tier: a builder only.
    Ok((model, assigned.map(|a| a.tier)))
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

/// The tier a role needs. Builders and review lanes are worker tier; the code-review role, the security
/// review, the finding check and the second key are lead tier (the harness routes `reviewer`, `security` and
/// `lead` to the lead tier). A review lane is the one cheaper reviewer: it defaults to the worker tier unless
/// `lead_only` says it is not to be lowered (see [`lane_is_lead_only`]).
pub fn role_tier(role: RouteRole, lead_only: bool) -> RouteTier {
    match role {
        RouteRole::Builder => RouteTier::Worker,
        RouteRole::Review if !lead_only => RouteTier::Worker,
        RouteRole::Scout => RouteTier::Scout,
        RouteRole::CodeReview
        | RouteRole::SecurityReview
        | RouteRole::FindingCheck
        | RouteRole::SecondKey
        | RouteRole::Review => RouteTier::Lead,
    }
}

/// A review lane that is never lowered below the lead tier: the security lane (which a high-risk diff owes) and any
/// lane a repository rule locks. A free-form lane named `secrets` is not the security lane.
pub fn lane_is_lead_only(lane: &str, locked: bool) -> bool {
    locked || lane == crate::crew::SECURITY_LANE
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

/// Only one visible explicitly marked default is a valid ACP assignment target.
/// Builder selection retains its existing fallback through `preferred`.
pub(super) fn explicit_default(models: &[ModelInfo]) -> Option<&ModelInfo> {
    let mut defaults = models.iter().filter(|m| m.is_default);
    let one = defaults.next()?;
    defaults.next().is_none().then_some(one)
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
