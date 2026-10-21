use acacia_physics::{BlockPos, Vec3};

use super::{Ctx, Process, Want};
use crate::pathfind::{Goal, GotoOpts};
use crate::ActionError;

/// The target moved this far (blocks) from the goal's centre: re-plan to where it is now.
const RETARGET: f32 = 2.0;
/// Standing within range, start moving again only once the target is this much further.
const SLACK: f32 = 1.5;
/// Wait after the pilot gave up on the target before trying again.
const RETRY_TICKS: u64 = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A player by name.
    Player(String),
    /// Any tracked entity by runtime id.
    Entity(u64),
}

/// Keeps within `range` blocks of a tracked entity: holds control while the target is tracked (moving
/// towards it or standing), yields while it is not, or for a while after failing to reach it. Needs
/// [`BotConfig::trackers`](crate::BotConfig)' entity tracking on a server.
pub struct Follow {
    target: Target,
    range: f32,
    opts: GotoOpts,
    centre: Option<BlockPos>,
    holding: bool,
    tick: u64,
    retry_at: u64,
}

impl Follow {
    pub fn player(name: impl Into<String>, range: f32) -> Self {
        Self::new(Target::Player(name.into()), range)
    }

    pub fn new(target: Target, range: f32) -> Self {
        Self { target, range, opts: GotoOpts::default(), centre: None, holding: false, tick: 0, retry_at: 0 }
    }

    pub fn with_opts(mut self, opts: GotoOpts) -> Self {
        self.opts = opts;
        self
    }

    pub fn target(&self) -> &Target {
        &self.target
    }
}

impl Process for Follow {
    fn name(&self) -> &str {
        "follow"
    }

    fn want(&mut self, cx: &Ctx) -> Want {
        self.tick = cx.tick;
        let Some(feet) = cx.look.find(&self.target) else {
            (self.centre, self.holding) = (None, false);
            return Want::Yield;
        };
        let d = dist(cx.pos, feet);
        self.holding = d <= self.range || (self.holding && d <= self.range + SLACK);
        if self.holding {
            return Want::Hold;
        }
        if cx.tick < self.retry_at {
            return Want::Yield;
        }
        let cell = [feet[0].floor() as i32, (feet[1] + 0.01).floor() as i32, feet[2].floor() as i32];
        let centre = match self.centre {
            Some(c) if dist(c.map(|v| v as f32), cell.map(|v| v as f32)) <= RETARGET => c,
            _ => *self.centre.insert(cell),
        };
        Want::Go(Goal::Near(centre, (self.range - 1.0).max(1.0)))
    }

    fn opts(&self) -> GotoOpts {
        self.opts.clone()
    }

    fn failed(&mut self, e: &ActionError) -> bool {
        tracing::debug!(%e, target = ?self.target, "cannot reach the followed target; waiting");
        (self.retry_at, self.centre) = (self.tick + RETRY_TICKS, None);
        false
    }
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}
