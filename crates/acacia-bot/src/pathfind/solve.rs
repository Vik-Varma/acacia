//! Solving a leap by rollout: the jump timing, coast and takeoff yaw that land it in fewest ticks without
//! a hazard, ceiling or hard fall, and for `Human` under every perturbation of the profile. The planner
//! solves from a node's momentum, the follower again from the live state when a leap drifts.

use acacia_physics::{BlockPos, PlayerState, WorldView};

use super::control::centre;
use super::leap::{JumpAt, LeapCtl, MAX_LEAP_TICKS};
use super::profile::{Aim, MovementProfile, Perturbation};
use super::rollout::{self, Body};
use crate::movement::{Controls, Rollout};

/// Last tick a jump is tried on; later ones have run off any edge.
const MAX_TAKEOFF: u8 = 12;
/// Latest coast tried on an overshoot (air ticks before letting go of forward).
const MAX_COAST: u8 = 8;
/// Falls up to 3 blocks do no damage.
const MAX_SAFE_FALL: f32 = 3.0;
/// Seed of the planned aim's jitter stream.
const AIM_SEED: u64 = 0x5eed;

/// Which leaps a solve tries.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shape {
    /// Takeoff yaws (degrees off the target line).
    pub yaws: &'static [f32],
    pub air_aim: bool,
    /// Also try running off the edge without a jump.
    pub drop: bool,
}

pub(crate) const STRAIGHT: Shape = Shape { yaws: &[0.0], air_aim: false, drop: false };
/// Off-line takeoff yaws and steering in the air, for leaps that miss straight (diagonals).
pub(crate) const WIDE: Shape = Shape { yaws: &[0.0, 11.25, -11.25, 22.5, -22.5], air_aim: true, drop: false };

pub(crate) struct Solved {
    /// Not yet started.
    pub ctl: LeapCtl,
    /// Worst ticks over the perturbations.
    pub ticks: u32,
    /// Least distance from the landing feet to the target cell's sides.
    pub margin: f32,
    /// The nominal run: its bodies and end state.
    pub bodies: Vec<Body>,
    pub end: PlayerState,
}

pub(crate) struct Solver<'a, W: WorldView + ?Sized> {
    pub w: &'a W,
    /// Cells the body must not touch.
    pub avoid: &'a dyn Fn(BlockPos) -> bool,
    /// Clearance kept from avoided cells.
    pub pad: f32,
    pub profile: MovementProfile,
    /// Check the profile's perturbations (planning); off when re-solving live.
    pub perturb: bool,
    /// Controls held before the first tick (a leap leaves the yaw alone in the air).
    pub held: Controls,
    pub rollouts: u32,
}

struct Attempt {
    /// The controller as it started.
    ctl: LeapCtl,
    takeoff: Option<u16>,
    bodies: Vec<Body>,
    end: PlayerState,
    clean: bool,
}

impl Attempt {
    fn last(&self) -> Body {
        self.bodies[self.bodies.len() - 1]
    }

    fn ticks(&self) -> u32 {
        self.bodies.len() as u32 - 1
    }
}

fn margin(b: &Body) -> f32 {
    let f = |v: f32| (v - v.floor()).min(v.floor() + 1.0 - v);
    f(b.pos[0]).min(f(b.pos[2]))
}

impl<W: WorldView + ?Sized> Solver<'_, W> {
    fn attempt(&mut self, start: &Rollout, ctl: LeapCtl) -> Attempt {
        self.rollouts += 1;
        let (mut r, mut running) = (start.clone(), ctl);
        let trace = rollout::run(&mut r, &mut running, self.held, u32::from(MAX_LEAP_TICKS) + 1, self.w);
        let clean = trace.landed && self.clean(&trace.bodies, ctl.wet);
        Attempt { ctl, takeoff: running.takeoff(), bodies: trace.bodies, end: r.st, clean }
    }

    /// No wall bump while rising, no ceiling, no hard fall into a dry landing, nothing avoided touched.
    fn clean(&self, bodies: &[Body], wet: bool) -> bool {
        bodies.iter().all(|b| {
            !(b.h_collision && b.vel[1] >= 0.0) && !b.bonk && (wet || b.fall <= MAX_SAFE_FALL) && !self.touches(b)
        })
    }

    fn touches(&self, b: &Body) -> bool {
        let r = 0.3 + self.pad;
        // No pad below the feet: a leap over lava level with the floor passes above it.
        let lo = [b.pos[0] - r, b.pos[1], b.pos[2] - r].map(|v| v.floor() as i32);
        let hi = [b.pos[0] + r, b.pos[1] + 1.8 + self.pad, b.pos[2] + r].map(|v| v.floor() as i32);
        (lo[0]..=hi[0]).any(|x| (lo[1]..=hi[1]).any(|y| (lo[2]..=hi[2]).any(|z| (self.avoid)([x, y, z]))))
    }

    /// The fastest clean leap from `start` to the cell `target` (feet `feet`, water if `wet`).
    pub fn solve(&mut self, start: &Rollout, target: BlockPos, feet: f32, wet: bool, shape: Shape) -> Option<Solved> {
        let mut best: Option<Solved> = None;
        for &yaw in shape.yaws {
            let mut ctl = LeapCtl::new(target, feet, JumpAt::Never, None, Aim::new(self.profile, AIM_SEED));
            (ctl.wet, ctl.yaw_off, ctl.air_aim) = (wet, yaw, shape.air_aim);
            // The air time barely depends on the takeoff, so the earliest takeoff that lands is the fastest.
            let (mut jumped, mut last) = (None, None);
            for k in 0..=MAX_TAKEOFF {
                let mut c = ctl;
                c.jump = JumpAt::Tick(k);
                let a = self.attempt(start, c);
                // A jump due after the edge never comes: the later ones neither.
                let Some(takeoff) = a.takeoff else { break };
                if last.replace(takeoff) == Some(takeoff) {
                    continue;
                }
                jumped = self.land(start, a).and_then(|a| self.judge(start, a));
                if jumped.is_some() {
                    break;
                }
            }
            let dropped = if shape.drop {
                let a = self.attempt(start, ctl);
                self.land(start, a).and_then(|a| self.judge(start, a))
            } else {
                None
            };
            for s in [jumped, dropped].into_iter().flatten() {
                if best.as_ref().is_none_or(|b| (s.ticks, -s.margin) < (b.ticks, -b.margin)) {
                    best = Some(s);
                }
            }
        }
        best
    }

    /// `a` if it landed, else the coast that lands it when it went long (binary search: a later coast
    /// flies farther).
    fn land(&mut self, start: &Rollout, a: Attempt) -> Option<Attempt> {
        if a.clean {
            return Some(a);
        }
        let ctl = a.ctl;
        let (from, goal) = ([start.st.pos[0], start.st.pos[2]], centre(ctl.target));
        let goal_along = along(from, ctl.target, goal);
        let short = |b: &Attempt| along(from, ctl.target, [b.last().pos[0], b.last().pos[2]]) < goal_along - 0.5;
        if short(&a) {
            return None;
        }
        let (mut lo, mut hi) = (0, i32::from(MAX_COAST));
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let mut c = ctl;
            c.coast = Some(mid as u8);
            let b = self.attempt(start, c);
            if b.clean {
                return Some(b);
            }
            if short(&b) { lo = mid + 1 } else { hi = mid - 1 }
        }
        None
    }

    /// The nominal landing `a` under the profile's perturbations: worst ticks, least margin, or `None`
    /// if one of them fails.
    fn judge(&mut self, start: &Rollout, a: Attempt) -> Option<Solved> {
        let (mut ticks, mut least) = (a.ticks(), margin(&a.last()));
        let perturbations = if self.perturb { self.profile.perturbations() } else { &[] };
        for p in perturbations.iter().filter(|p| **p != Perturbation::NOMINAL) {
            let (from, ctl) = perturbed(start, a.ctl, a.takeoff, p);
            let b = self.attempt(&from, ctl);
            if !b.clean {
                return None;
            }
            (ticks, least) = (ticks.max(b.ticks()), least.min(margin(&b.last())));
        }
        Some(Solved { ctl: a.ctl, ticks, margin: least, bodies: a.bodies, end: a.end })
    }

    /// A leap that drifted, solved again from the live state: an airborne one by its coast, else afresh.
    pub fn retry(&mut self, ctl: &LeapCtl, live: &Rollout, shape: Shape) -> Option<(LeapCtl, Vec<Body>)> {
        if ctl.airborne() {
            let mut c = *ctl;
            let coasts = [None].into_iter().chain((0..=MAX_COAST).map(Some));
            return coasts
                .map(|coast| {
                    c.coast = coast;
                    self.attempt(live, c)
                })
                .find(|a| a.clean)
                .map(|a| (a.ctl, a.bodies));
        }
        let shape = Shape { drop: shape.drop || ctl.jump == JumpAt::Never, ..shape };
        self.perturb = false;
        self.profile = ctl.aim.profile;
        let s = self.solve(live, ctl.target, ctl.feet, ctl.wet, shape)?;
        Some((s.ctl, s.bodies))
    }
}

/// How far along the line from `from` to `target`'s centre `at` lies (blocks).
fn along(from: [f32; 2], target: BlockPos, at: [f32; 2]) -> f32 {
    let [tx, tz] = centre(target);
    let (sx, sz) = (tx - from[0], tz - from[1]);
    ((at[0] - from[0]) * sx + (at[1] - from[1]) * sz) / sx.hypot(sz).max(1e-3)
}

/// The start and controller of one perturbed execution.
fn perturbed(start: &Rollout, mut ctl: LeapCtl, takeoff: Option<u16>, p: &Perturbation) -> (Rollout, LeapCtl) {
    if let Some(k) = takeoff.filter(|_| p.jump != 0) {
        ctl.jump = JumpAt::Tick((i32::from(k) + i32::from(p.jump)).clamp(0, i32::from(u8::MAX)) as u8);
    }
    ctl.aim.bias = p.yaw;
    let mut from = start.clone();
    if p.start != 0.0 {
        let [tx, tz] = centre(ctl.target);
        let (dx, dz) = (tx - from.st.pos[0], tz - from.st.pos[2]);
        let len = dx.hypot(dz).max(1e-3);
        from.st.pos[0] -= dx / len * p.start;
        from.st.pos[2] -= dz / len * p.start;
    }
    (from, ctl)
}
