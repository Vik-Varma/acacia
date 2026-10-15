//! Floors on a leap's cost before it is solved (lazy leaps, docs/pathfind-plan.md "Core fixes"): how far the feet
//! can get in each tick from a node's momentum, simulated once per start speed on open flat ground.

use std::collections::HashMap;

use acacia_physics::constants::DEFAULT_MOVEMENT_SPEED;
use acacia_physics::{Aabb, BlockPhysics, BlockPos, PlayerState, WorldView};

use super::costs::{DANGER_NEAR, HUMAN_JUMP};
use super::leap::MAX_LEAP_TICKS;
use super::leap_moves::LeapCand;
use super::node::Momentum;
use super::profile::MovementProfile;
use super::terrain::{Blocks, Terrain};
use crate::movement::{Controls, Rollout};

/// Default ground friction: other floors (ice) carry more speed than the flat run below assumes.
const DEFAULT_FRICTION: f32 = 0.6;
/// Fastest horizontal travel in one tick any leap reaches without effects (`tests::hops`), for floors the flat run
/// does not cover.
pub(crate) const LEAP_VMAX: f32 = 0.7;

/// A stone floor under y 0, air above.
struct Flat;

impl WorldView for Flat {
    fn block_collisions(&self, pos: BlockPos, out: &mut Vec<Aabb>) {
        if pos[1] < 0 {
            out.push(Aabb::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0));
        }
    }

    fn block(&self, pos: BlockPos) -> BlockPhysics {
        if pos[1] < 0 { BlockPhysics::solid(DEFAULT_FRICTION) } else { BlockPhysics::AIR }
    }
}

/// Ticks of sprinting before holding jump that the reach tries (`None`: never jump).
const JUMP_AFTER: [Option<usize>; 7] = [None, Some(0), Some(1), Some(2), Some(3), Some(5), Some(8)];

/// Farthest the feet get from the start by each tick, sprinting straight from a node's speed and jumping at any of
/// [`JUMP_AFTER`].
pub(crate) struct Reach {
    by_tick: Vec<f32>,
}

impl Reach {
    /// From momentum `mom` (attributes and effects from `base`), on default-friction ground.
    fn new(mom: &Momentum, base: &PlayerState) -> Self {
        let mut st = mom.synth([0, 0, 0], base);
        let speed = st.vel[0].hypot(st.vel[2]);
        (st.pos, st.vel, st.yaw) = ([0.5, 0.0, 0.5], [speed, st.vel[1], 0.0], -90.0);
        let mut by_tick = vec![0.0f32; usize::from(MAX_LEAP_TICKS) + 1];
        for after in JUMP_AFTER {
            let mut r = Rollout::new(st.clone());
            for (k, d) in by_tick.iter_mut().enumerate().skip(1) {
                let jump = after.is_some_and(|a| k > a);
                r.step(&Controls { forward: 1.0, sprint: true, jump, yaw: -90.0, ..Controls::default() }, &Flat);
                *d = d.max(r.st.pos[0] - 0.5);
            }
        }
        Self { by_tick }
    }

    /// Fewest ticks to cover `d` blocks (the last tick if never).
    fn ticks(&self, d: f32) -> f32 {
        self.by_tick.iter().position(|&x| x >= d).unwrap_or(self.by_tick.len() - 1) as f32
    }
}

/// A search's reaches by start speed and flags (rest nodes all share one).
#[derive(Default)]
pub(crate) struct Reaches(HashMap<(i16, i16, i16, u8), Reach>);

impl Reaches {
    pub fn get(&mut self, mom: &Momentum, base: &PlayerState) -> &Reach {
        let v = mom.velocity();
        let speed = (v[0].hypot(v[2]) * 4096.0).round() as i16;
        self.0.entry((speed, mom.vel[1], mom.off[1], mom.flags)).or_insert_with(|| Reach::new(mom, base))
    }
}

/// Least cost the leap to `c` from `p` (moving as `mom`) can have: ticks for the feet to reach the target cell at
/// the fastest run (one tick of slack), `Human`'s hesitation when it must jump, and the hazard term.
pub(crate) fn leap_floor<B: Blocks>(t: &Terrain<B>, p: BlockPos, mom: &Momentum, c: &LeapCand, reach: &Reach, base: &PlayerState, profile: MovementProfile) -> f32 {
    let [x, _, z] = mom.feet(p);
    let (lo, hi) = ([c.q[0] as f32, c.q[2] as f32], [c.q[0] as f32 + 1.0, c.q[2] as f32 + 1.0]);
    let d = (lo[0] - x).max(x - hi[0]).max(0.0).hypot((lo[1] - z).max(z - hi[1]).max(0.0));
    let ticks = if t.block([p[0], p[1] - 1, p[2]]).friction > DEFAULT_FRICTION {
        d / (LEAP_VMAX * (base.movement_attribute / DEFAULT_MOVEMENT_SPEED).max(1.0))
    } else {
        (reach.ticks(d) - 1.0).max(0.0)
    };
    let jumps = !c.to.wet && c.q[1] >= p[1];
    let hesitation = if profile == MovementProfile::Human && jumps { HUMAN_JUMP } else { 0.0 };
    let danger = if t.danger_near(c.q) { DANGER_NEAR } else { 0.0 };
    ticks + hesitation + danger
}
