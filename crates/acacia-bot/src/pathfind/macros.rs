//! Macro successors: moves found by simulating their controller from the node's momentum
//! (`docs/pathfind-plan.md` "Macro moves"), cached across nodes ([`RolloutCache`]) and capped by a rollout
//! tick budget.

use std::cell::{Cell, RefCell};

use acacia_physics::{BlockPos, PlayerState};

use super::cache::RolloutCache;
use super::cell::AVOID;
use super::costs::{DANGER_NEAR, HUMAN_JUMP, RISK, RISK_MARGIN};
use super::hop;
use super::leap::JumpAt;
use super::leap_moves;
use super::mac::Macro;
use super::moves::{Edge, MoveKind};
use super::node::{Entry, Momentum, run_entry};
use super::profile::MovementProfile;
use super::solve::{Solved, Solver};
use super::terrain::{Blocks, Spot, SpotKind, Terrain};
use crate::movement::{Controls, Rollout};

/// What one search spent on macro moves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MacroStats {
    pub rollouts: u32,
    /// Ticks simulated.
    pub ticks: u64,
    /// Move families answered by the cache, and those simulated.
    pub hits: u32,
    pub misses: u32,
    /// The tick budget ran out: later nodes got classic moves only.
    pub exhausted: bool,
}

/// What the search's macro moves need.
pub(crate) struct Macros<'a> {
    pub profile: MovementProfile,
    /// Gap leaps (`SearchOpts::parkour`).
    pub leaps: bool,
    /// Sprint-jumps along flat ground (`SearchOpts::hops`).
    pub hops: bool,
    /// The live state: attributes, effects and equipment of every rollout.
    pub base: &'a PlayerState,
    pub cache: RefCell<&'a mut RolloutCache>,
    /// Rollout ticks the search may simulate (`SearchOpts::max_rollout_ticks`).
    pub budget: u64,
    pub stats: Cell<MacroStats>,
}

pub(crate) fn successors<B: Blocks>(t: &Terrain<B>, p: BlockPos, from: Spot, (entry, mom): (Entry, &Momentum), m: &Macros, out: &mut Vec<Edge>) {
    if entry == Entry::Air || from.kind != SpotKind::Stand || from.wet || from.made != 0 {
        return;
    }
    let mut stats = m.stats.get();
    if stats.ticks >= m.budget {
        stats.exhausted = true;
        return m.stats.set(stats);
    }
    let start = Rollout::new(mom.synth(p, m.base));
    let avoid = |q: BlockPos| t.cell(q).has(AVOID);
    let safe = |q: BlockPos| t.spot(q).is_some_and(|s| s.kind == SpotKind::Stand && !s.wet && s.feet == q[1] as f32);
    let mut solver = Solver::new(t, &avoid, &safe, m.profile, true, Controls::default());
    solver.budget = m.budget - stats.ticks;
    let mut cache = m.cache.borrow_mut();
    let cache: &mut RolloutCache = &mut cache;
    let counts = cache.counts;
    if m.leaps {
        for (q, to, s) in leap_moves::leaps(t, p, from, mom, &start, &mut solver, cache) {
            push_leap(t, out, q, to, s, MoveKind::Parkour, m.profile);
        }
    }
    for (q, to, r) in hop::runs(t, p, from, (entry, mom), &start, &mut solver, cache) {
        push(t, out, q, to, MoveKind::Run, r.ticks as f32, &r.end, Macro::Run(r.ctl));
    }
    if m.hops
        && !solver.spent()
        && let Entry::Run(d, _) = entry
        && let Some((q, to, s)) = cache.hops.get_or(t, p, (*mom, d), &mut cache.counts, || hop::hop(t, p, from, d, &start, &mut solver))
    {
        push_leap(t, out, q, to, s, MoveKind::Hop, m.profile);
    }
    stats.rollouts += solver.rollouts;
    stats.ticks += solver.ticks;
    stats.hits += cache.counts.hits - counts.hits;
    stats.misses += cache.counts.misses - counts.misses;
    stats.exhausted |= solver.spent();
    m.stats.set(stats);
}

fn push_leap<B: Blocks>(t: &Terrain<B>, out: &mut Vec<Edge>, pos: BlockPos, to: Spot, s: Solved, kind: MoveKind, profile: MovementProfile) {
    let mut cost = s.ticks as f32;
    if profile == MovementProfile::Human {
        cost += RISK * (1.0 - s.margin / RISK_MARGIN).max(0.0) + if s.ctl.jump == JumpAt::Never { 0.0 } else { HUMAN_JUMP };
    }
    push(t, out, pos, to, kind, cost, &s.end, Macro::Leap(s.ctl));
}

#[allow(clippy::too_many_arguments)]
fn push<B: Blocks>(t: &Terrain<B>, out: &mut Vec<Edge>, pos: BlockPos, to: Spot, kind: MoveKind, mut cost: f32, end: &PlayerState, mac: Macro) {
    if t.danger_near(pos) {
        cost += DANGER_NEAR;
    }
    let (entry, mom) = if to.wet { (Entry::Rest, Momentum::rest(to.feet, pos)) } else { (run_entry(end.vel), Momentum::snap(end, pos)) };
    out.push(Edge { pos, spot: to, kind, cost, work: Default::default(), entry, mom, mac: Some(mac) });
}
