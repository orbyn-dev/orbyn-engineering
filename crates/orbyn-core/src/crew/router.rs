//! Router v0 (docs/crew.md, "Router v0"): no model, no I/O. Given a brief and the engines
//! this machine has configured, it filters, scores what is left from seeded defaults,
//! honours an explicit engine from the lead or the person, and returns a `RouteDecision`.
//!
//! An explicit engine wins but is still filtered. If it fails, the router refuses with the
//! reason and never swaps in another engine. The only inputs are the brief (which has no
//! field for the answer key) and the engine views, so nothing of the key can reach a decision.

use orbyn_contracts::{
    Brief, BriefAssigner, BriefKind, CrewAuthor, CrewError, CrewHash, CrewId, CrewObject, CrewRole,
    CrewTimestamp, CrewToken, RejectionWhy, RouteAssigner, RouteDecision, RouteEffort, RouteReason,
    RouteRejection, RouteRequested, RouteRole, RouteScore,
};

use super::classes::PathClasses;
use super::filters::{Admitted, Want, class_screen, rejection, screen, short};
use super::score::{EvalHistory, Scored, score};
use super::tiers::{RouterConfig, reviews, role_for_kind, role_tier};
use super::view::EngineView;

/// Everything one routing needs besides the engines.
pub struct RouteInput<'a> {
    pub brief: &'a Brief,
    pub run_id: CrewId,
    pub created_at: CrewTimestamp,
    /// The role, when the caller names one. Otherwise the brief's kind decides.
    pub role: Option<RouteRole>,
    /// The reviewer lane, exactly when the role is `review`.
    pub lane: Option<CrewToken>,
    /// The engine that did the work being reviewed, for the other-vendor rule.
    pub author_engine: Option<CrewToken>,
    /// The person's default engine. It breaks a tie and adds no points.
    pub default_engine: Option<String>,
    /// What the repository's data classes say about the brief's files, read at the commit the task starts from. `None` is no
    /// classification at all (a pure route that reads no repository).
    pub classes: Option<&'a PathClasses>,
}

/// Why nothing was chosen. Names what was asked for, so the lead can say why.
#[derive(Clone, Debug)]
pub struct RouteRefusal {
    pub requested: Option<RouteRequested>,
    pub rejected: Vec<RouteRejection>,
    pub message: String,
}

#[derive(Debug)]
pub enum RouteFailure {
    Invalid(CrewError),
    Refused(RouteRefusal),
}

fn kind_name(kind: BriefKind) -> String {
    super::wire_name(&kind)
}

fn core_author() -> CrewAuthor {
    CrewAuthor {
        role: CrewRole::Core,
        engine: CrewToken::new("orbyn").expect("a token"),
        member: None,
    }
}

fn code(text: &str) -> CrewToken {
    CrewToken::new(text).expect("a fixed reason code")
}

fn reason(name: &str, text: impl Into<String>) -> RouteReason {
    RouteReason {
        code: code(name),
        text: short(text),
    }
}

fn score_text(view: &EngineView, scored: &Scored, kind: BriefKind) -> String {
    let basis = match &scored.score.pass_rate {
        Some(rate) => format!("measured, {} samples", rate.samples),
        None => "seeded default".to_owned(),
    };
    let headroom = match view.used_percent {
        Some(used) => format!("{used}% used"),
        None => "usage not reported".to_owned(),
    };
    format!(
        "{}: {} for {} briefs = pass rate {} ({basis}) and headroom {} ({headroom}).",
        view.name,
        scored.score.score,
        kind_name(kind),
        scored.pass_permille,
        scored.headroom_permille
    )
}

/// The classes that applied and the file they were read from, so the decision can be checked against it later.
fn classes_text(classes: &PathClasses) -> String {
    let file = &classes.file;
    let from = match (&file.commit, &file.blob) {
        (Some(commit), Some(blob)) => {
            format!(" Rules from .orbyn/crew.json at commit {commit}, blob {blob}.")
        }
        (Some(commit), None) => format!(" Rules read at commit {commit}."),
        _ => String::new(),
    };
    let from = match &classes.pinned {
        Some(pinned) => format!(
            "{from} Also read at {} (the default branch), commit {}.",
            pinned.name, pinned.commit
        ),
        None => from,
    };
    let every = classes
        .unknown_why()
        .map_or_else(String::new, |why| format!(" Every class counted: {why}."));
    let differ = classes
        .differs()
        .map_or_else(String::new, |words| format!(" {words}"));
    format!(
        "Path classes: {}.{every}{from}{differ}",
        classes.names().join(", ")
    )
}

fn refusal(
    requested: Option<RouteRequested>,
    rejected: Vec<RouteRejection>,
    message: String,
) -> RouteFailure {
    RouteFailure::Refused(RouteRefusal {
        requested,
        rejected,
        message,
    })
}

/// Moves every admitted engine of `family` into `rejected`, for the other-vendor rule.
fn refuse_same_vendor(
    admitted: &mut Vec<Admitted<'_>>,
    rejected: &mut Vec<RouteRejection>,
    family: &str,
) {
    admitted.retain(|a| {
        if a.view.family != family {
            return true;
        }
        rejected.push(rejection(
            a.view,
            RejectionWhy::SameEngineRefused,
            format!(
                "{} wrote the work, so another vendor reviews it.",
                a.view.name
            ),
        ));
        false
    });
}

pub fn route(
    input: &RouteInput<'_>,
    engines: &[EngineView],
    config: &RouterConfig,
    history: &EvalHistory,
) -> Result<RouteDecision, RouteFailure> {
    let brief = input.brief;
    let role = input.role.unwrap_or_else(|| role_for_kind(brief.kind));
    match (role == RouteRole::Review, input.lane.is_some()) {
        (true, false) => {
            return Err(RouteFailure::Invalid(CrewError::new(
                orbyn_contracts::ErrorCode::InvalidObject,
                "lane: a review route names its lane",
            )));
        }
        (false, true) => {
            return Err(RouteFailure::Invalid(CrewError::new(
                orbyn_contracts::ErrorCode::InvalidObject,
                "lane: only a review route has one",
            )));
        }
        _ => {}
    }
    let want = Want::of(brief, role_tier(role));
    let request = brief.engine.as_ref();
    let requested = request.map(|r| RouteRequested {
        engine: r.id.clone(),
        model: r.model.as_ref().map(|m| short(m.as_str())),
    });

    let mut sorted: Vec<&EngineView> = engines.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    let mut admitted: Vec<Admitted<'_>> = Vec::new();
    let mut rejected: Vec<RouteRejection> = Vec::new();
    for view in sorted {
        // A data class comes before every other filter and before scoring.
        if let Some(classes) = input.classes
            && let Err(why) = class_screen(view, classes, engines)
        {
            rejected.push(why);
            continue;
        }
        let named = request
            .filter(|r| r.id == view.id)
            .and_then(|r| r.model.as_ref())
            .map(CrewToken::as_str);
        match screen(view, &want, named, config) {
            Ok(one) => admitted.push(one),
            Err(why) => rejected.push(why),
        }
    }
    if let Some(r) = request
        && !engines.iter().any(|v| v.id == r.id)
    {
        rejected.push(RouteRejection {
            engine: r.id.clone(),
            why: RejectionWhy::NotInstalled,
            text: short(format!("{} isn't configured on this machine.", r.id)),
        });
    }

    let author_family = input.author_engine.as_ref().map(|author| {
        engines
            .iter()
            .find(|v| v.id == *author)
            .map_or_else(|| author.as_str().to_owned(), |v| v.family.clone())
    });
    let mut compared = false;
    if reviews(role)
        && let Some(family) = &author_family
    {
        // A second key and the independent run that confirms a finding are never on the same vendor.
        let require = matches!(role, RouteRole::SecondKey | RouteRole::FindingCheck);
        match request {
            None if require || admitted.iter().any(|a| a.view.family != *family) => {
                refuse_same_vendor(&mut admitted, &mut rejected, family);
            }
            Some(_) if require => refuse_same_vendor(&mut admitted, &mut rejected, family),
            _ => {}
        }
        compared = true;
    }

    let default = input.default_engine.as_deref();
    let is_default = |a: &Admitted<'_>| Some(a.view.id.as_str()) == default;
    let mut ranked: Vec<(Admitted<'_>, Scored)> = admitted
        .into_iter()
        .map(|a| {
            let scored = score(&a.view.id, brief.kind, a.view.used_percent, history);
            (a, scored)
        })
        .collect();
    ranked.sort_by(|(a, sa), (b, sb)| {
        sb.score
            .score
            .cmp(&sa.score.score)
            .then_with(|| is_default(b).cmp(&is_default(a)))
            .then_with(|| a.view.id.cmp(&b.view.id))
    });

    let chosen_at = match request {
        Some(r) => ranked.iter().position(|(a, _)| a.view.id == r.id),
        None => (!ranked.is_empty()).then_some(0),
    };
    let Some(at) = chosen_at else {
        let (why, message) = match request {
            Some(r) => {
                let own: Vec<RouteRejection> = rejected
                    .iter()
                    .filter(|x| x.engine == r.id)
                    .cloned()
                    .collect();
                let text = own.first().map_or_else(
                    || format!("{} can't take this brief.", r.id),
                    |x| x.text.as_str().to_owned(),
                );
                (own, text)
            }
            None => {
                let message = match input.classes {
                    Some(classes)
                        if !rejected.is_empty()
                            && rejected.iter().all(|x| x.why == RejectionWhy::PathClass) =>
                    {
                        if classes.file.invalid() {
                            format!("Every engine is blocked: {}", classes.file.problem_text())
                        } else {
                            format!(
                                "No engine is allowed by every rule that covers this brief ({}).",
                                classes.names().join(", ")
                            )
                        }
                    }
                    _ => "No engine on this machine can take this brief.".to_owned(),
                };
                (rejected, message)
            }
        };
        return Err(refusal(requested, why, message));
    };

    let (chosen, chosen_score) = &ranked[at];
    let tie_won_by_default = request.is_none()
        && ranked.len() > 1
        && is_default(chosen)
        && ranked[1].1.score.score == chosen_score.score.score;
    let chosen_same_vendor = compared
        && author_family
            .as_deref()
            .is_some_and(|family| chosen.view.family == family);

    let mut reasons = vec![reason(
        "role",
        format!(
            "{} role, {} tier.",
            super::wire_name(&role),
            super::wire_name(&want.tier)
        ),
    )];
    match (request, brief.engine.as_ref().map(|r| r.assigned_by)) {
        (Some(r), Some(by)) => {
            let who = if by == BriefAssigner::Person {
                "you"
            } else {
                "the lead"
            };
            let tag = r
                .tag
                .as_ref()
                .map_or_else(|| r.id.to_string(), |t| t.to_string());
            reasons.push(reason(
                if by == BriefAssigner::Person {
                    "assigned_by_person"
                } else {
                    "assigned_by_lead"
                },
                format!("Assigned by {who} ({tag})."),
            ));
        }
        _ => reasons.push(reason(
            "highest_score",
            format!(
                "Highest score for {} briefs: {}.",
                kind_name(brief.kind),
                chosen_score.score.score
            ),
        )),
    }
    if tie_won_by_default {
        reasons.push(reason(
            "default_engine_tiebreak",
            "Tied on score; your default engine wins.",
        ));
    }
    if let Some(classes) = input.classes.filter(|c| c.applies()) {
        reasons.push(reason("path_classes", classes_text(classes)));
    }
    reasons.push(reason(
        "model",
        format!(
            "{} fills the {} tier.",
            chosen.model,
            super::wire_name(&chosen.tier)
        ),
    ));
    reasons.push(reason("effort_medium", "Effort stays at medium."));
    if chosen_same_vendor {
        reasons.push(reason("same_engine_as_author", "Same engine as author."));
    }
    if chosen_score.score.used_defaults {
        reasons.push(reason(
            "seeded_defaults",
            "No measured results yet, so pass rates are seeded defaults.",
        ));
    }
    for (rank, (one, scored)) in ranked.iter().enumerate() {
        if reasons.len() >= 32 {
            break;
        }
        reasons.push(reason(
            &format!("score_{}", rank + 1),
            score_text(one.view, scored, brief.kind),
        ));
    }

    let assigner = match brief.engine.as_ref().map(|r| r.assigned_by) {
        Some(BriefAssigner::Person) => RouteAssigner::Person,
        Some(BriefAssigner::Lead) => RouteAssigner::Lead,
        None => RouteAssigner::Router,
    };
    let scores: Vec<RouteScore> = ranked.iter().map(|(_, s)| s.score.clone()).collect();
    let mut decision = RouteDecision {
        v: 0,
        id: CrewHash::new(format!("sha256:{}", "0".repeat(64))).expect("a hash"),
        crew_id: brief.crew_id.clone(),
        author: core_author(),
        created_at: input.created_at.clone(),
        run_id: input.run_id.clone(),
        brief: brief.id.clone(),
        engine: chosen.view.id.clone(),
        model: short(chosen.model.clone()),
        effort: RouteEffort::Medium,
        tier: chosen.tier,
        role,
        assigned_by: assigner,
        requested,
        reasons,
        rejected,
        scores,
        lane: input.lane.clone(),
    };
    let id = CrewObject::RouteDecision(decision.clone())
        .computed_id()
        .map_err(RouteFailure::Invalid)?;
    decision.id = id;
    decision.validate().map_err(RouteFailure::Invalid)?;
    Ok(decision)
}
