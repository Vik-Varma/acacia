//! Controllers: one per macro move. The planner runs it in rollouts and the follower runs the same one
//! live (`follow_macro`), so what was planned is what runs.

use acacia_physics::BlockPos;

use super::profile::{Aim, MovementProfile};
use super::rollout::Body;
use crate::movement::Controls;

/// Ticks a leap may take, run-up included.
const MAX_LEAP_TICKS: u16 = 40;
/// Feet within this of the target's floor count as standing on it.
const LAND_EPS: f32 = 1e-3;
/// Below the target's floor by this much, a leap has failed.
const FELL: f32 = 1.5;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JumpAt {
    /// On the last tick walking on would still be on the ground.
    Edge,
    /// On the move's k-th tick (0 = first), or the first one on the ground after it.
    Tick(u8),
    /// Once this far (blocks) along the move from where it started.
    Along(f32),
}

/// Sprints at `target`, jumps and lands standing in its cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeapCtl {
    pub target: BlockPos,
    /// Feet height standing on the target.
    pub feet: f32,
    pub jump: JumpAt,
    /// Let go of forward after this many ticks in the air.
    pub coast: Option<u8>,
    pub aim: Aim,
    t: u16,
    air: u16,
    grounded: bool,
    jumped: bool,
    origin: Option<[f32; 2]>,
}

impl LeapCtl {
    pub fn new(target: BlockPos, feet: f32, jump: JumpAt, coast: Option<u8>, aim: Aim) -> Self {
        Self { target, feet, jump, coast, aim, t: 0, air: 0, grounded: false, jumped: false, origin: None }
    }

    fn centre(&self) -> [f32; 2] {
        [self.target[0] as f32 + 0.5, self.target[2] as f32 + 0.5]
    }

    fn landed(&self, b: &Body) -> bool {
        [b.pos[0].floor() as i32, b.pos[2].floor() as i32] == [self.target[0], self.target[2]] && (b.pos[1] - self.feet).abs() < LAND_EPS
    }

    fn along(&self, origin: [f32; 2], b: &Body) -> f32 {
        let [tx, tz] = self.centre();
        let (sx, sz) = (tx - origin[0], tz - origin[1]);
        ((b.pos[0] - origin[0]) * sx + (b.pos[2] - origin[1]) * sz) / sx.hypot(sz).max(1e-3)
    }
}

impl Controller for LeapCtl {
    fn step(&mut self, b: &Body, peek: &mut dyn FnMut(&Controls) -> Body, c: &mut Controls) -> Phase {
        let origin = *self.origin.get_or_insert([b.pos[0], b.pos[2]]);
        if b.on_ground && self.air > 0 {
            return if self.landed(b) { Phase::Done } else { Phase::Failed };
        }
        // A jump pressed on the ground that did not lift off.
        let stalled = self.jumped && b.on_ground;
        if stalled || self.t >= MAX_LEAP_TICKS || b.pos[1] < self.feet - FELL {
            return Phase::Failed;
        }
        let tick = self.t;
        self.t += 1;
        // Air before the first ground contact (a start still settling) is no leap.
        self.grounded |= b.on_ground;
        if !b.on_ground && self.grounded {
            self.air += 1;
        }
        (c.strafe, c.sneak, c.jump, c.pitch) = (0.0, false, false, 0.0);
        if b.on_ground {
            let [tx, tz] = self.centre();
            c.yaw = self.aim.turn(b.yaw, (b.pos[0] - tx).atan2(tz - b.pos[2]).to_degrees());
        }
        let coasting = self.coast.is_some_and(|k| self.air >= u16::from(k));
        (c.forward, c.sprint) = if coasting { (0.0, false) } else { (1.0, true) };
        if b.on_ground {
            c.jump = match self.jump {
                JumpAt::Edge => !peek(c).on_ground,
                JumpAt::Tick(k) => tick >= u16::from(k),
                JumpAt::Along(x) => self.along(origin, b) >= x,
            };
            self.jumped = c.jump;
        }
        Phase::Going
    }
}

/// A macro move's controller as stored on its path node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Macro {
    Leap(LeapCtl),
}

impl Macro {
    pub fn profile(&self) -> MovementProfile {
        match self {
            Macro::Leap(l) => l.aim.profile,
        }
    }
}

impl Controller for Macro {
    fn step(&mut self, b: &Body, peek: &mut dyn FnMut(&Controls) -> Body, c: &mut Controls) -> Phase {
        match self {
            Macro::Leap(l) => l.step(b, peek, c),
        }
    }
}
