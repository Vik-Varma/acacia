use super::{Ctx, Process, Want};
use crate::pathfind::{Goal, GotoOpts};
use crate::ActionError;

/// Drives to a goal; done on arrival, failed when the pilot gives up. Pre-empted, it re-plans from where it
/// regains control.
pub struct Goto {
    goal: Goal,
    opts: GotoOpts,
}

impl Goto {
    pub fn new(goal: Goal) -> Self {
        Self::with(goal, GotoOpts::default())
    }

    pub fn with(goal: Goal, opts: GotoOpts) -> Self {
        Self { goal, opts }
    }

    pub fn goal(&self) -> &Goal {
        &self.goal
    }
}

impl Process for Goto {
    fn name(&self) -> &str {
        "goto"
    }

    fn want(&mut self, _: &Ctx) -> Want {
        Want::Go(self.goal.clone())
    }

    fn opts(&self) -> GotoOpts {
        self.opts.clone()
    }

    fn arrived(&mut self) -> bool {
        true
    }

    fn failed(&mut self, _: &ActionError) -> bool {
        true
    }
}
