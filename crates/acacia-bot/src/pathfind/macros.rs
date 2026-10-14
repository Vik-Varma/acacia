//! Macro successors: moves found by simulating their controller from the node's momentum
//! (`docs/pathfind-plan.md` "Macro moves").

use std::cell::Cell;

use acacia_physics::{BlockPos, PlayerState};

use super::cell::{AVOID, Rect};
use super::control::Macro;
use super::costs::{DANGER_NEAR, HUMAN_JUMP, RISK, RISK_MARGIN};
use super::leap::JumpAt;
use super::moves::{Edge, MoveKind, offset};
use super::node::{Entry, Momentum, run_entry};
use super::profile::MovementProfile;
use super::solve::{STRAIGHT, Solved, Solver};
use super::terrain::{Blocks, Spot, SpotKind, Terrain};
use crate::movement::{Controls, Rollout};

/// Farthest cardinal leap (blocks to the landing cell): a 4-gap needs a run-up of two blocks from rest.
const MAX_LEAP: i32 = 7;
const CARDINALS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// What the search's macro moves need.
pub(crate) struct Macros<'a> {
    pub profile: MovementProfile,
    /// Gap leaps (`SearchOpts::parkour`).
    pub leaps: bool,
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
    let (pad, held) = (m.profile.hazard_pad(), Controls::default());
    let mut solver = Solver { w: t, avoid: &avoid, pad, profile: m.profile, perturb: true, held, rollouts: 0 };
    if m.leaps {
        leaps(t, p, from, &start, &mut solver, out);
    }
    m.rollouts.set(m.rollouts.get() + solver.rollouts);
}

/// Leaps over at least one gap cell onto a dry floor, along each cardinal lane until a cell blocks it
/// (cells behind a blocked one are shadowed: SwarmBot's prefilter).
fn leaps<B: Blocks>(t: &Terrain<B>, p: BlockPos, from: Spot, start: &Rollout, solver: &mut Solver<Terrain<B>>, out: &mut Vec<Edge>) {
    for (dx, dz) in CARDINALS {
        let start = &facing(start, dx as f32, dz as f32);
        let mut gap = false;
        for k in 1..=MAX_LEAP {
            let c = offset(p, dx * k, 0, dz * k);
            if gap && let Some(to) = t.spot(c).filter(|s| s.kind == SpotKind::Stand && !s.wet) {
                let s = solver.solve(start, c, to.feet, false, STRAIGHT);
                push(t, out, c, to, s, solver.profile);
            }
            let lane = Rect::lane(dx, dz);
            if !t.body_clear(c, lane, from.feet) || !t.body_clear(c, lane, from.feet + 1.0) {
                break;
            }
            gap |= t.spot(c).is_none() && t.spot(offset(c, 0, 1, 0)).is_none();
        }
    }
}

/// `start` turned towards `(dx, dz)` if it stands still: a standing player faces where it goes next (a `Human`
/// aim eases from the facing, so it shapes the leap).
fn facing(start: &Rollout, dx: f32, dz: f32) -> Rollout {
    let mut r = start.clone();
    if r.st.vel[0] == 0.0 && r.st.vel[2] == 0.0 {
        r.st.yaw = (-dx).atan2(dz).to_degrees();
    }
    r
}

fn push<B: Blocks>(t: &Terrain<B>, out: &mut Vec<Edge>, pos: BlockPos, to: Spot, s: Option<Solved>, profile: MovementProfile) {
    let Some(s) = s else { return };
    let mut cost = s.ticks as f32;
    if profile == MovementProfile::Human {
        cost += RISK * (1.0 - s.margin / RISK_MARGIN).max(0.0) + if s.ctl.jump == JumpAt::Never { 0.0 } else { HUMAN_JUMP };
    }
    if t.danger_near(pos) {
        cost += DANGER_NEAR;
    }
    let (entry, mom) = if to.wet { (Entry::Rest, Momentum::rest(to.feet, pos)) } else { (run_entry(s.end.vel), Momentum::snap(&s.end, pos)) };
    out.push(Edge { pos, spot: to, kind: MoveKind::Parkour, cost, work: Default::default(), entry, mom, mac: Some(Macro::Leap(s.ctl)) });
}
