//! Scoring (docs/crew.md, Router v0, step 2): the engine's pass rate for the brief's kind,
//! then its quota headroom. The person's default engine breaks a tie and adds no points.
//! Every number is an integer in permille, because a protocol object holds integers only.

use std::collections::HashMap;

use orbyn_contracts::{BriefKind, PassRate, RouteScore};

/// What an engine is assumed to pass until a measured result exists. Not a claim about any
/// engine: the same for all of them, and every score built on it says `used_defaults`.
pub const SEED_PASS_PERMILLE: u32 = 500;
/// Headroom when an engine reports no limits: halfway, so silence neither helps nor hurts.
pub const UNREPORTED_HEADROOM_PERMILLE: u32 = 500;

const PASS_WEIGHT: u32 = 7;
const HEADROOM_WEIGHT: u32 = 3;

/// Measured pass rates by engine and brief kind, each with its sample size. Empty in v0:
/// slice 4d and 4g feed it from real runs, and the router already prefers a measured rate.
#[derive(Clone, Debug, Default)]
pub struct EvalHistory(HashMap<(String, BriefKind), PassRate>);

impl EvalHistory {
    pub fn record(&mut self, engine: &str, kind: BriefKind, rate: PassRate) {
        self.0.insert((engine.to_owned(), kind), rate);
    }

    pub fn rate(&self, engine: &str, kind: BriefKind) -> Option<&PassRate> {
        self.0.get(&(engine.to_owned(), kind))
    }
}

/// Free quota in permille: 1000 for an unused window, 0 for a full one.
fn headroom(used_percent: Option<u32>) -> u32 {
    used_percent.map_or(UNREPORTED_HEADROOM_PERMILLE, |used| {
        1000 - used.min(100) * 10
    })
}

/// One engine's score, with the numbers behind it for the reason text.
#[derive(Clone, Debug)]
pub struct Scored {
    pub score: RouteScore,
    pub pass_permille: u32,
    pub headroom_permille: u32,
}

pub fn score(
    engine: &orbyn_contracts::CrewToken,
    kind: BriefKind,
    used_percent: Option<u32>,
    history: &EvalHistory,
) -> Scored {
    let measured = history.rate(engine.as_str(), kind);
    let pass_permille = measured.map_or(SEED_PASS_PERMILLE, |rate| rate.permille.min(1000));
    let headroom_permille = headroom(used_percent);
    Scored {
        score: RouteScore {
            engine: engine.clone(),
            score: (pass_permille * PASS_WEIGHT + headroom_permille * HEADROOM_WEIGHT) / 10,
            used_defaults: measured.is_none(),
            pass_rate: measured.cloned(),
        },
        pass_permille,
        headroom_permille,
    }
}
