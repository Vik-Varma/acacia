//! Running and hopping: `Run` sprints up to three cells along a direction to arrive with its momentum
//! (to the edge of a gap, or one cell onto a lane of four or more); `Hop` sprint-jumps on along the
//! direction of a `Run` entry and lands wherever the jump comes down on the same level.

use acacia_physics::{BlockPos, PlayerState};

use super::control::RunCtl;
use super::moves::offset;
use super::node::{DIRS, Entry};
use super::profile::Aim;
use super::solve::{AIM_SEED, STRAIGHT, Solved, Solver};
use super::terrain::{Blocks, EPS, Spot, SpotKind, Terrain};
use crate::movement::Rollout;

/// Farthest run (cells), reaching the edge of a gap.
const RUN_REACH: i32 = 3;
/// Flat cells ahead that make a lane worth running onto.
const LANE: i32 = 4;
/// A hop is aimed this far along its direction; it lands well short of it.
const HOP_AIM: i32 = 8;

pub(crate) struct Ran {
    pub ctl: RunCtl,
    pub ticks: u32,
    pub end: PlayerState,
}

/// A dry floor at the start's level.
fn level<B: Blocks>(t: &Terrain<B>, q: BlockPos, from: Spot) -> Option<Spot> {
    t.spot(q).filter(|s| s.kind == SpotKind::Stand && !s.wet && (s.feet - from.feet).abs() < EPS)
}

/// The runs from `p`: along every direction from rest, along the running direction otherwise.
pub(crate) fn runs<B: Blocks>(t: &Terrain<B>, p: BlockPos, from: Spot, entry: Entry, start: &Rollout, solver: &mut Solver<Terrain<B>>) -> Vec<(BlockPos, Spot, Ran)> {
    let dirs: &[(i32, i32)] = match entry {
        Entry::Run(d, _) => &DIRS[usize::from(d)..=usize::from(d)],
        _ => &DIRS,
    };
    let mut out = Vec::new();
    for &(dx, dz) in dirs {
        let at = |k: i32| offset(p, dx * k, 0, dz * k);
        let flat = (1..=LANE).take_while(|&k| level(t, at(k), from).is_some()).count() as i32;
        let gap = |q: BlockPos| t.spot(q).is_none() && t.spot(offset(q, 0, 1, 0)).is_none();
        // One cell onto a lane, or right up to the edge of a gap.
        let k = match flat {
            LANE => 1,
            1..=RUN_REACH if gap(at(flat + 1)) => flat,
            _ => continue,
        };
        let Some(to) = level(t, at(k), from) else { continue };
        let ctl = RunCtl::new(at(k), to.feet, Aim::new(solver.profile, AIM_SEED));
        let (bodies, end, clean) = solver.run(start, &mut { ctl }, false);
        if clean {
            out.push((at(k), to, Ran { ctl, ticks: bodies.len() as u32 - 1, end }));
        }
    }
    out
}

/// The hop on from a `Run(d)` entry: where a sprint-jump along `d` lands on the same level, if it lands there
/// cleanly.
pub(crate) fn hop<B: Blocks>(t: &Terrain<B>, p: BlockPos, from: Spot, entry: Entry, start: &Rollout, solver: &mut Solver<Terrain<B>>) -> Option<(BlockPos, Spot, Solved)> {
    let Entry::Run(d, _) = entry else { return None };
    let (dx, dz) = DIRS[usize::from(d)];
    let down = solver.hop_landing(start, offset(p, dx * HOP_AIM, 0, dz * HOP_AIM), from.feet);
    let cell = [down.pos[0].floor() as i32, p[1], down.pos[2].floor() as i32];
    let to = level(t, cell, from).filter(|_| down.on_ground && cell != p)?;
    if let Some(s) = solver.hop(start, cell, to.feet) {
        return Some((cell, to, s));
    }
    // Came down too near the cell's edge for the profile's slack: coast into the cell before it.
    let back = offset(cell, -dx, 0, -dz);
    let to = level(t, back, from).filter(|_| back != p)?;
    Some((back, to, solver.solve(start, back, to.feet, false, STRAIGHT).ok()?))
}
