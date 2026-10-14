//! Per-tick path following: turns a path into [`Controls`], network-free so it can run against the
//! physics simulation headlessly.

mod steer;
mod work;

use acacia_physics::{Vec3, WorldView};

use super::follow_macro::{MacroReport, MacroRunner, MacroStep};
use super::moves::MoveKind;
use super::profile::MovementProfile;
use super::shortcut;
use super::PathNode;
use crate::movement::{Controls, Rollout};

/// No progress towards the current node for this long means stuck (1 s).
const STUCK_TICKS: u32 = 20;
const MIN_PROGRESS: f32 = 0.05;
/// Farther than this from the current segment (horizontally) means knocked off the path.
const OFF_PATH: f32 = 1.5;

/// What the follower knows about the player this tick.
#[derive(Debug, Clone, Copy)]
pub struct Sense {
    pub pos: Vec3,
    pub on_ground: bool,
    pub in_water: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowStatus {
    Moving,
    Arrived,
    Stuck,
    OffPath,
    /// Standing at the start of a move whose [`Work`](super::Work) must be done first: do
    /// [`Follower::pending_work`], then call [`Follower::work_done`].
    Work,
    /// A macro move cannot finish from here: re-plan without stopping (the controls are left as they were).
    Diverged,
}

pub struct Follower {
    path: Vec<PathNode>,
    index: usize,
    /// The current node's work is done.
    worked: bool,
    /// Start of the current segment: the previous node's centre, or where the bot started.
    origin: Vec3,
    last_pos: Option<Vec3>,
    best_dist: f32,
    since_progress: u32,
    tolerance: f32,
    macros: MacroRunner,
    /// Take any-angle shortcuts, moving like this.
    shortcuts: Option<MovementProfile>,
    /// Path index a shortcut was last looked for from.
    tried: Option<usize>,
}

fn center(n: &PathNode) -> Vec3 {
    [n.pos[0] as f32 + 0.5, n.feet, n.pos[2] as f32 + 0.5]
}

fn hdist(a: Vec3, b: Vec3) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

fn dir(a: Vec3, b: Vec3) -> (i32, i32) {
    ((b[0] - a[0]).round() as i32, (b[2] - a[2]).round() as i32)
}

impl Follower {
    /// `tolerance`: how close (horizontally, blocks) to the final node's centre counts as arrived.
    pub fn new(path: Vec<PathNode>, start: Vec3, tolerance: f32) -> Self {
        let macros = MacroRunner::default();
        Self { path, index: 0, worked: false, origin: start, last_pos: None, best_dist: f32::INFINITY, since_progress: 0, tolerance, macros, shortcuts: None, tried: None }
    }

    /// On flat ground, run straight at nodes further on when that beats following the nodes between
    /// (moving like `profile`; `None`: never).
    pub fn shortcuts(mut self, profile: Option<MovementProfile>) -> Self {
        self.shortcuts = profile;
        self
    }

    /// How the last finished macro move went, once.
    pub fn take_macro_report(&mut self) -> Option<MacroReport> {
        self.macros.take_report()
    }

    pub fn path(&self) -> &[PathNode] {
        &self.path
    }

    /// Index of the node being walked to.
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn remaining(&self) -> usize {
        self.path.len() - self.index
    }

    /// The nodes still ahead, the current target first.
    pub fn ahead(&self) -> &[PathNode] {
        &self.path[self.index..]
    }

    /// `live`: a fork of the live simulation, which macro moves need (without it they are steered
    /// classically); `w` the world it runs in.
    pub fn tick(&mut self, s: &Sense, live: Option<&Rollout>, w: &dyn WorldView, c: &mut Controls) -> FollowStatus {
        let vel = self.last_pos.map_or([0.0; 3], |l| [s.pos[0] - l[0], s.pos[1] - l[1], s.pos[2] - l[2]]);
        self.last_pos = Some(s.pos);
        if self.path.is_empty() {
            c.stop();
            return FollowStatus::Arrived;
        }
        if !self.macros.active() {
            self.advance(s);
        }
        // A finished macro move hands over to the next one on the same tick.
        loop {
            self.try_shortcut(live, w);
            match self.macros.tick(&self.path, self.index, live, w, c) {
                MacroStep::Idle => break,
                MacroStep::Going => return FollowStatus::Moving,
                MacroStep::Diverged => return FollowStatus::Diverged,
                MacroStep::Done(last) => {
                    self.index = last;
                    self.pass_node();
                }
            }
        }
        c.pitch = 0.0;
        let mut node = self.path[self.index];
        if self.macros.finished(self.index) {
            // Landed on the final node: only settling there is left.
            node.kind = MoveKind::Walk;
        }
        let target = center(&node);
        if self.off_path(s.pos, target) {
            c.stop();
            return FollowStatus::OffPath;
        }
        let last = self.index + 1 == self.path.len();
        let dist = hdist(s.pos, target);
        let speed = vel[0].hypot(vel[2]);
        if self.has_pending(self.index) {
            return self.approach_work(s, c, speed);
        }
        if last && dist < self.tolerance && speed < 0.03 && self.reached(&node, s, self.tolerance) {
            c.stop();
            return FollowStatus::Arrived;
        }
        let dist3 = dist.hypot(target[1] - s.pos[1]);
        if dist3 < self.best_dist - MIN_PROGRESS {
            (self.best_dist, self.since_progress) = (dist3, 0);
        } else {
            self.since_progress += 1;
            if self.since_progress > STUCK_TICKS {
                c.stop();
                return FollowStatus::Stuck;
            }
        }
        self.steer(s, c, &node, target, dist, speed, last);
        FollowStatus::Moving
    }

    /// Once per node, with nothing running: start a shortcut to a node further on if one beats the nodes between.
    fn try_shortcut(&mut self, live: Option<&Rollout>, w: &dyn WorldView) {
        let (Some(profile), Some(live)) = (self.shortcuts, live) else { return };
        if self.macros.active() || self.tried.replace(self.index) == Some(self.index) {
            return;
        }
        if let Some((ctl, predicted, last)) = shortcut::find(&self.path, self.index, live, w, profile) {
            self.macros.begin(ctl, predicted, last);
        }
    }

    /// Moves on to the next node, if any.
    fn pass_node(&mut self) {
        if self.index + 1 < self.path.len() {
            self.origin = center(&self.path[self.index]);
            (self.index, self.worked) = (self.index + 1, false);
            (self.best_dist, self.since_progress) = (f32::INFINITY, 0);
        }
    }

    fn advance(&mut self, s: &Sense) {
        let before = self.index;
        while self.index + 1 < self.path.len() && !self.has_pending(self.index) {
            let node = self.path[self.index];
            let next = self.path[self.index + 1];
            let straight = next.kind == MoveKind::Walk && dir(self.origin, center(&node)) == dir(center(&node), center(&next));
            if !self.reached(&node, s, if straight { 0.45 } else { 0.25 }) {
                break;
            }
            self.pass_node();
        }
        // Overshot onto a later node (momentum, a fall): continue from there, but not past work.
        let end = self.index + self.standing_ahead().len();
        let window = (self.index + 1..end.min(self.index + 4)).rev();
        for j in window {
            let n = self.path[j];
            let feet_cell = [s.pos[0].floor() as i32, s.pos[2].floor() as i32];
            if feet_cell == [n.pos[0], n.pos[2]] && (s.pos[1] - n.feet).abs() < 0.3 && (s.on_ground || s.in_water) {
                self.origin = center(&self.path[j - 1]);
                self.index = j;
                self.worked = false;
                break;
            }
        }
        if self.index != before {
            (self.best_dist, self.since_progress) = (f32::INFINITY, 0);
        }
    }

    fn reached(&self, n: &PathNode, s: &Sense, radius: f32) -> bool {
        let h = hdist(s.pos, center(n));
        let dy = s.pos[1] - n.feet;
        match n.kind {
            MoveKind::ClimbUp => dy > -0.05 && h < 0.5,
            MoveKind::ClimbDown => dy < 0.1 && h < 0.5,
            MoveKind::Ascend => h < radius && dy > -0.25 && dy < 0.6,
            MoveKind::Parkour | MoveKind::Descend | MoveKind::Down => h < radius.max(0.35) && dy.abs() < 0.5 && (s.on_ground || s.in_water),
            MoveKind::Pillar => h < 0.5 && dy > -0.1 && dy < 0.6 && s.on_ground,
            MoveKind::Walk | MoveKind::Swim | MoveKind::Run | MoveKind::Hop => h < radius && dy.abs() < 0.6,
        }
    }

    fn off_path(&self, pos: Vec3, target: Vec3) -> bool {
        let (ax, az) = (self.origin[0], self.origin[2]);
        let (sx, sz) = (target[0] - ax, target[2] - az);
        let len2 = sx * sx + sz * sz;
        let t = if len2 > 0.0 { (((pos[0] - ax) * sx + (pos[2] - az) * sz) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let off = (pos[0] - ax - sx * t).hypot(pos[2] - az - sz * t);
        let (lo, hi) = (self.origin[1].min(target[1]), self.origin[1].max(target[1]));
        off > OFF_PATH || pos[1] < lo - 1.5 || pos[1] > hi + 2.5
    }
}
