//! What a search may do, and what it returns.

use std::sync::Arc;
use std::time::Duration;

use super::costs;
use super::favour::Favour;
use super::goal::Speed;
use super::kit::Kit;
use super::macros::MacroStats;
use super::profile::MovementProfile;
use super::PathNode;

#[derive(Debug, Clone)]
pub struct SearchOpts {
    pub max_nodes: usize,
    pub budget: Duration,
    /// Allow gap jumps.
    pub parkour: bool,
    /// Break blocks in the way (costed by break time with the best tool in `kit`).
    pub allow_dig: bool,
    /// Place scaffold blocks from `kit` to cross gaps and to pillar up.
    pub allow_bridge: bool,
    /// Open wooden doors, fence gates and trapdoors in the way.
    pub open_doors: bool,
    /// Tools and blocks to plan with; [`Bot::goto`](crate::Bot::goto) fills it from the inventory.
    pub kit: Kit,
    /// How macro moves are planned and run.
    pub profile: MovementProfile,
    /// Plan macro moves (leaps found by simulation) instead of the classic gap jump.
    pub macros: bool,
    /// Sprint-jump along flat ground (macro moves). [`Bot::goto`](crate::Bot::goto) turns it off at food 6 or less.
    pub hops: bool,
    /// Rollout ticks the search may simulate for macro moves; once spent, nodes get classic moves only.
    pub max_rollout_ticks: u64,
    /// Cells of the path being replaced, made cheaper to keep a re-plan on it; the navigator fills it.
    pub favour: Option<Arc<Favour>>,
}

impl Default for SearchOpts {
    fn default() -> Self {
        Self {
            max_nodes: 200_000,
            budget: Duration::from_secs(2),
            parkour: true,
            allow_dig: false,
            allow_bridge: false,
            open_doors: true,
            kit: Kit::default(),
            profile: MovementProfile::Optimal,
            macros: true,
            hops: true,
            max_rollout_ticks: 400_000,
            favour: None,
        }
    }
}

impl SearchOpts {
    /// The heuristic's speed: macro moves sprint-jump, and change height on the way.
    pub(crate) fn speed(&self) -> Speed {
        if self.macros {
            Speed { per_block: costs::macro_ticks_per_block(self.profile), startup: costs::MACRO_STARTUP, overlap: true }
        } else {
            Speed::CLASSIC
        }
    }
}

#[derive(Debug, Clone)]
pub struct Path {
    /// Nodes after the start, in order.
    pub nodes: Vec<PathNode>,
    /// False when the search ran out of budget or options: `nodes` then leads to the node that
    /// looked closest to the goal and still has a way on (never into a pit).
    pub complete: bool,
    /// The search saw every cell it could reach, all loaded, without finding the goal: no path exists
    /// with these moves (until the world changes).
    pub unreachable: bool,
    /// Estimated ticks (undiscounted by [`SearchOpts::favour`]).
    pub cost: f32,
    /// Nodes expanded.
    pub visited: usize,
    pub macro_stats: MacroStats,
}
