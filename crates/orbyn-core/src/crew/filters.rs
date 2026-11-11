//! The hard filters. The first one an engine fails is the reason it is rejected, in the
//! order of docs/crew.md (Router v0, step 1): installed, signed in, compatible, able to
//! be managed and to ask for approvals when the brief writes, able to meet the brief's
//! `needs`, a model for the tier, and quota left.

use orbyn_contracts::{
    AuthState, Brief, BriefEffect, CrewShortText, EngineCapabilities, LeaseMode, ModelInfo,
    RejectionWhy, RouteRejection, RouteTier,
};

use super::tiers::{RouterConfig, Unfit, model_for_tier, model_named, tier_of_model};
use super::view::EngineView;

/// What the brief asks of an engine, read once.
pub struct Want {
    pub writes: bool,
    pub needs: Vec<String>,
    pub image: bool,
    pub tier: RouteTier,
}

impl Want {
    pub fn of(brief: &Brief, tier: RouteTier) -> Self {
        let needs: Vec<String> = brief
            .needs
            .iter()
            .flatten()
            .map(|need| need.as_str().to_owned())
            .collect();
        Self {
            writes: brief.allowed_effects.contains(&BriefEffect::WriteLeased)
                || brief.files.iter().any(|f| f.mode == LeaseMode::Write),
            image: needs.iter().any(|need| need == "image"),
            needs,
            tier,
        }
    }
}

/// An engine that passed every filter, with the model it would run.
#[derive(Clone, Debug)]
pub struct Admitted<'a> {
    pub view: &'a EngineView,
    pub model: String,
    pub tier: RouteTier,
}

/// Cuts a sentence to the protocol's text limit.
pub fn short(text: impl Into<String>) -> CrewShortText {
    let text: String = text.into();
    let cut: String = text.chars().take(1000).collect();
    CrewShortText::new(if cut.is_empty() { "-".to_owned() } else { cut })
        .expect("1 to 1000 characters")
}

pub fn rejection(view: &EngineView, why: RejectionWhy, text: impl Into<String>) -> RouteRejection {
    RouteRejection {
        engine: view.id.clone(),
        why,
        text: short(text),
    }
}

/// A need the engine itself meets, or `None` for one the router does not know.
fn engine_need(name: &str, caps: &EngineCapabilities) -> Option<bool> {
    Some(match name {
        "mcp" => caps.mcp,
        "streaming" => caps.streaming,
        "approvals" => caps.approvals,
        "resume" => caps.resume,
        "sandbox" => caps.sandbox,
        "managed" => caps.managed,
        "goal_control" => caps.goal_control,
        "decline_feedback" => caps.decline_feedback,
        _ => return None,
    })
}

fn unfit_text(
    view: &EngineView,
    tier: RouteTier,
    unfit: &Unfit,
    asked: Option<&str>,
) -> (RejectionWhy, String) {
    let name = &view.name;
    match unfit {
        Unfit::NoModels => (
            RejectionWhy::MissingCapability,
            format!("{name} lists no models on this machine."),
        ),
        Unfit::Named => (
            RejectionWhy::MissingCapability,
            format!(
                "{name} has no model named {} here.",
                asked.unwrap_or("that")
            ),
        ),
        Unfit::Input(kind) => (
            RejectionWhy::MissingCapability,
            format!("None of {name}'s models takes {kind} input."),
        ),
        Unfit::Tier(RouteTier::Lead) => (
            RejectionWhy::TierBelowLead,
            format!("{name} has no lead-tier model here, so it can't take this role."),
        ),
        Unfit::Tier(_) => (
            RejectionWhy::MissingCapability,
            format!(
                "{name} has no model for the {} tier here.",
                if tier == RouteTier::Scout {
                    "scout"
                } else {
                    "worker"
                }
            ),
        ),
    }
}

/// Runs every filter in order. `named` is the model the lead or person asked for on this
/// engine, which still has to be listed and still has to meet the role's tier.
pub fn screen<'a>(
    view: &'a EngineView,
    want: &Want,
    named: Option<&str>,
    config: &RouterConfig,
) -> Result<Admitted<'a>, RouteRejection> {
    let name = &view.name;
    if view.turned_off {
        return Err(rejection(
            view,
            RejectionWhy::NotInstalled,
            format!("{name} is turned off in Settings."),
        ));
    }
    if !view.installed {
        return Err(rejection(
            view,
            RejectionWhy::NotInstalled,
            format!("{name} isn't installed."),
        ));
    }
    match view.auth {
        AuthState::LoggedIn | AuthState::NotApplicable => {}
        AuthState::LoggedOut => {
            return Err(rejection(
                view,
                RejectionWhy::NotSignedIn,
                format!("{name} isn't signed in."),
            ));
        }
        AuthState::Unknown => {
            return Err(rejection(
                view,
                RejectionWhy::NotSignedIn,
                format!("{name}'s sign-in couldn't be confirmed."),
            ));
        }
    }
    if let Some(message) = &view.broken {
        let note: String = message.chars().take(300).collect();
        return Err(rejection(
            view,
            RejectionWhy::CompatibilityBroken,
            format!("{name}'s installed version is known to be broken: {note}"),
        ));
    }
    if want.writes {
        if !view.capabilities.managed {
            return Err(rejection(
                view,
                RejectionWhy::NotManaged,
                format!("{name} can't be gated by Orbyn, so it can't write."),
            ));
        }
        if !view.capabilities.approvals {
            return Err(rejection(
                view,
                RejectionWhy::NoApprovals,
                format!("{name} can't ask for approval, so it can't write."),
            ));
        }
    }
    for need in want.needs.iter().filter(|need| *need != "image") {
        match engine_need(need, &view.capabilities) {
            Some(true) => {}
            Some(false) => {
                return Err(rejection(
                    view,
                    RejectionWhy::MissingCapability,
                    format!("{name} lacks {need}, which the brief needs."),
                ));
            }
            None => {
                return Err(rejection(
                    view,
                    RejectionWhy::MissingCapability,
                    format!("The brief needs {need}, which is not a capability the router knows."),
                ));
            }
        }
    }
    let models: &[ModelInfo] = view.models.as_deref().unwrap_or(&[]);
    let model = match named {
        Some(asked) => model_named(models, asked, want.image),
        None => model_for_tier(config, &view.family, models, want.tier, want.image),
    }
    .map_err(|unfit| {
        let (why, text) = unfit_text(view, want.tier, &unfit, named);
        rejection(view, why, text)
    })?;
    let tier = tier_of_model(config, &view.family, model);
    if want.tier == RouteTier::Lead && tier != Some(RouteTier::Lead) {
        return Err(rejection(
            view,
            RejectionWhy::TierBelowLead,
            format!(
                "{} is below the lead tier, which this role needs.",
                model.id
            ),
        ));
    }
    if view.used_percent.is_some_and(|used| used >= 100) {
        return Err(rejection(
            view,
            RejectionWhy::QuotaExhausted,
            format!("{name} has no usage left in its current window."),
        ));
    }
    Ok(Admitted {
        view,
        model: model.id.clone(),
        tier: tier.unwrap_or(want.tier),
    })
}
