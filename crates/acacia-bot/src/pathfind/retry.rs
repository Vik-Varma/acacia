//! Re-solving a leap live, for the follower: from the state the bot is really in, not the planned one.

use acacia_physics::{BlockPos, LiquidKind, Vec3, WorldView};

use super::leap::{JumpAt, LeapCtl};
use super::rollout::Body;
use super::solve::{MAX_COAST, MAX_TAKEOFF, Shape, Solver};
use crate::movement::Rollout;

impl<W: WorldView + ?Sized> Solver<'_, W> {
    /// A leap that drifted, solved again from the live state: an airborne one by its coast, else by any
    /// takeoff and coast. Of the landings, the one arriving nearest the planned horizontal velocity `want`
    /// wins (then the fastest), so the moves after it still start as planned.
    pub fn retry(&mut self, ctl: &LeapCtl, live: &Rollout, shape: Shape, want: Vec3) -> Option<(LeapCtl, Vec<Body>)> {
        let coasts = || [None].into_iter().chain((0..=MAX_COAST).map(Some));
        let mut tries: Vec<LeapCtl> = Vec::new();
        if ctl.airborne() {
            tries.extend(coasts().map(|coast| {
                let mut c = *ctl;
                c.coast = coast;
                c
            }));
        } else {
            let fresh = |yaw: f32, jump: JumpAt, coast: Option<u8>| {
                let mut c = LeapCtl::new(ctl.target, ctl.feet, jump, coast, ctl.aim);
                (c.wet, c.yaw_off, c.air_aim, c.slack) = (ctl.wet, yaw, shape.air_aim, ctl.slack);
                c
            };
            for &yaw in shape.yaws {
                for k in 0..=MAX_TAKEOFF {
                    // A jump due after the edge never comes: the later ones neither.
                    if self.attempt(live, fresh(yaw, JumpAt::Tick(k), Some(0))).takeoff.is_none() {
                        break;
                    }
                    tries.extend(coasts().map(|coast| fresh(yaw, JumpAt::Tick(k), coast)));
                }
                if shape.drop || ctl.jump == JumpAt::Never {
                    tries.extend(coasts().map(|coast| fresh(yaw, JumpAt::Never, coast)));
                }
            }
        }
        let off = |v: Vec3| ((v[0] - want[0]).hypot(v[2] - want[2]) * 100.0).round() as u32;
        let landed = tries.into_iter().map(|c| self.attempt(live, c)).filter(|a| a.clean);
        landed.min_by_key(|a| (off(a.end.vel), a.ticks())).map(|a| (a.ctl, a.bodies))
    }
}

/// What a live re-solve or shortcut must not touch, read from the physics view: lava and cobwebs.
pub(crate) fn live_hazard(w: &dyn WorldView, p: BlockPos) -> bool {
    let b = w.block(p);
    b.cobweb || b.liquid.is_some_and(|l| l.kind == LiquidKind::Lava)
}
