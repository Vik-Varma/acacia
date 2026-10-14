//! Controllers: one per macro move. The planner runs it in rollouts and the follower runs the same one
//! live (`follow_macro`), so what was planned is what runs.

use acacia_physics::BlockPos;

use super::leap::LeapCtl;
use super::profile::{Aim, MovementProfile};
use super::rollout::Body;
use crate::movement::Controls;

/// Feet within this of the target's floor count as standing on it.
pub(crate) const LAND_EPS: f32 = 1e-3;
/// Ticks a run may take to reach its cell.
const MAX_RUN_TICKS: u16 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Going,
    Done,
    Failed,
}

pub trait Controller {
    /// Sets `c` for the tick after body `b`. `peek` simulates that tick with other controls, committing nothing.
    fn step(&mut self, b: &Body, peek: &mut dyn FnMut(&Controls) -> Body, c: &mut Controls) -> Phase;
}

pub(crate) fn centre(p: BlockPos) -> [f32; 2] {
    [p[0] as f32 + 0.5, p[2] as f32 + 0.5]
}

/// Yaw (degrees) facing from `b` to `to` (xz).
pub(crate) fn yaw_to(b: &Body, to: [f32; 2]) -> f32 {
    (b.pos[0] - to[0]).atan2(to[1] - b.pos[2]).to_degrees()
}

/// On the ground in `target`'s column with feet at `feet`.
pub(crate) fn standing_on(b: &Body, target: BlockPos, feet: f32) -> bool {
    b.on_ground && b.cell() == [target[0], target[2]] && (b.pos[1] - feet).abs() < LAND_EPS
}

/// Sprints along the ground into `target`'s cell, arriving with the momentum of the run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunCtl {
    pub target: BlockPos,
    pub feet: f32,
    pub aim: Aim,
    t: u16,
}

impl RunCtl {
    pub fn new(target: BlockPos, feet: f32, aim: Aim) -> Self {
        Self { target, feet, aim, t: 0 }
    }
}

impl Controller for RunCtl {
    fn step(&mut self, b: &Body, _: &mut dyn FnMut(&Controls) -> Body, c: &mut Controls) -> Phase {
        if standing_on(b, self.target, self.feet) {
            return Phase::Done;
        }
        if !b.on_ground || self.t >= MAX_RUN_TICKS {
            return Phase::Failed;
        }
        self.t += 1;
        (c.strafe, c.sneak, c.jump, c.pitch, c.forward, c.sprint) = (0.0, false, false, 0.0, 1.0, true);
        c.yaw = self.aim.turn(b.yaw, yaw_to(b, centre(self.target)));
        Phase::Going
    }
}

/// A macro move's controller as stored on its path node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Macro {
    Leap(LeapCtl),
    Run(RunCtl),
}

impl Macro {
    pub fn profile(&self) -> MovementProfile {
        match self {
            Macro::Leap(l) => l.aim.profile,
            Macro::Run(r) => r.aim.profile,
        }
    }
}

impl Controller for Macro {
    fn step(&mut self, b: &Body, peek: &mut dyn FnMut(&Controls) -> Body, c: &mut Controls) -> Phase {
        match self {
            Macro::Leap(l) => l.step(b, peek, c),
            Macro::Run(r) => r.step(b, peek, c),
        }
    }
}
