//! Macro successors: moves found by simulating their controller from the node's momentum
//! (`docs/pathfind-plan.md` "Macro moves").

use std::cell::Cell;

use acacia_physics::{BlockPos, PlayerState};

use super::cell::AVOID;
use super::mac::Macro;
use super::costs::{DANGER_NEAR, HUMAN_JUMP, RISK, RISK_MARGIN};
use super::hop;
use super::leap::JumpAt;
use super::leap_moves;
use super::moves::{Edge, MoveKind};
use super::node::{Entry, Momentum, run_entry};
use super::profile::MovementProfile;
use super::solve::{Solved, Solver};
use super::terrain::{Blocks, Spot, SpotKind, Terrain};
use crate::movement::{Controls, Rollout};

/// What the search's macro moves need.
pub(crate) struct Macros<'a> {
    pub profile: MovementProfile,
    /// Gap leaps (`SearchOpts::parkour`).
    pub leaps: bool,
    /// Sprint-jumps along flat ground (`SearchOpts::hops`).
    pub hops: bool,
    /// The live state: attributes, effects and equipment of every rollout.
    pub base: &'a PlayerState,
    pub rollouts: Cell<u32>,
}

pub(crate) fn successors<B: Blocks>(t: &Terrain<B>, p: BlockPos, from: Spot, (entry, mom): (Entry, &Momentum), m: &Macros, out: &mut Vec<Edge>) {
    if entry == Entry::Air || from.kind != SpotKind::Stand || from.wet || from.made != 0 {
        return;
    }
    let start = Rollout::new(mom.synth(p, m.base));
    let avoid = |q: BlockPos| t.cell(q).has(AVOID);
    let safe = |q: BlockPos| t.spot(q).is_some_and(|s| s.kind == SpotKind::Stand && !s.wet && s.feet == q[1] as f32);
    let (pad, held) = (m.profile.hazard_pad(), Controls::default());
    let mut solver = Solver { w: t, avoid: &avoid, pad, safe: &safe, profile: m.profile, perturb: true, held, rollouts: 0 };
    if m.leaps {
        for (q, to, s) in leap_moves::leaps(t, p, from, &start, &mut solver) {
            push_leap(t, out, q, to, s, MoveKind::Parkour, m.profile);
        }
    }
    for (q, to, r) in hop::runs(t, p, from, entry, &start, &mut solver) {
        push(t, out, q, to, MoveKind::Run, r.ticks as f32, &r.end, Macro::Run(r.ctl));
    }
    if m.hops
        && let Some((q, to, s)) = hop::hop(t, p, from, entry, &start, &mut solver)
    {
        push_leap(t, out, q, to, s, MoveKind::Hop, m.profile);
    }
    m.rollouts.set(m.rollouts.get() + solver.rollouts);
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
