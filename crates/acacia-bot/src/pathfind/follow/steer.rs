//! Classic steering: the controls that walk, jump and sprint towards the current node.

use acacia_physics::Vec3;

use super::{Follower, Sense, center, dir};
use crate::movement::Controls;
use crate::pathfind::PathNode;
use crate::pathfind::moves::MoveKind;

/// Ground drag per tick (block friction 0.6 × 0.91): coasting from speed `v` covers `v·k/(1-k)`.
const GROUND_DRAG: f32 = 0.546;
/// Straight nodes ahead before sprinting pays off.
const SPRINT_RUN: usize = 3;

impl Follower {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn steer(&self, s: &Sense, c: &mut Controls, node: &PathNode, target: Vec3, dist: f32, speed: f32, last: bool) {
        let (dx, dz) = (target[0] - s.pos[0], target[2] - s.pos[2]);
        if dist > 0.05 {
            c.yaw = (-dx).atan2(dz).to_degrees();
        }
        c.strafe = 0.0;
        c.sneak = false;
        c.forward = if last {
            // Bedrock ground friction lets the bot coast: release the key once coasting covers the rest.
            let coast = speed * GROUND_DRAG / (1.0 - GROUND_DRAG);
            if dist < coast + 0.05 { 0.0 } else { 1.0 }
        } else if matches!(node.kind, MoveKind::ClimbUp | MoveKind::ClimbDown | MoveKind::Pillar | MoveKind::Down) && dist < 0.15 {
            0.0
        } else {
            1.0
        };
        let rise = node.feet - s.pos[1];
        c.jump = match node.kind {
            MoveKind::Ascend => rise > 0.3 && dist < 1.3,
            MoveKind::ClimbUp => true,
            MoveKind::Parkour => s.on_ground && self.along(s.pos) > 0.35,
            _ if s.in_water => rise > -0.3,
            _ => false,
        };
        let wanted = !s.in_water
            && match node.kind {
                MoveKind::Parkour | MoveKind::Run | MoveKind::Hop => true,
                MoveKind::Walk => self.straight_run() >= SPRINT_RUN || self.runs_into_macro(),
                _ => false,
            };
        // The sprint key only starts a sprint; Movement keeps it going until forward is released.
        c.sprint = wanted;
    }

    /// Distance travelled from the segment origin along the segment direction.
    fn along(&self, pos: Vec3) -> f32 {
        let t = center(&self.path[self.index]);
        let (sx, sz) = (t[0] - self.origin[0], t[2] - self.origin[2]);
        let len = sx.hypot(sz).max(1e-3);
        ((pos[0] - self.origin[0]) * sx + (pos[2] - self.origin[2]) * sz) / len
    }

    /// Consecutive walk nodes from the current one heading the same way.
    fn straight_run(&self) -> usize {
        let mut prev = self.origin;
        let heading = dir(prev, center(&self.path[self.index]));
        let mut run = 0;
        for n in &self.path[self.index..] {
            if n.kind != MoveKind::Walk || !n.work.is_empty() || dir(prev, center(n)) != heading {
                break;
            }
            prev = center(n);
            run += 1;
        }
        run
    }

    /// The next node is a macro move heading on the way the bot walks now: arriving at speed is a run-up.
    fn runs_into_macro(&self) -> bool {
        let Some(next) = self.path.get(self.index + 1).filter(|n| n.mac.is_some()) else { return false };
        let here = center(&self.path[self.index]);
        let unit = |(x, z): (i32, i32)| (x.signum(), z.signum());
        unit(dir(self.origin, here)) == unit(dir(here, center(next)))
    }
}
