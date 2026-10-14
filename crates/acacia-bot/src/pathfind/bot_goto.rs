//! The pathfinding methods on [`Bot`].

use acacia_physics::BlockPos;

use super::goal::Goal;
use super::goto::{GotoOpts, NavStatus, Navigator};
use crate::{ActionError, Bot};

impl Bot {
    /// Walks to `goal` (physics bots only). See [`Navigator`] for step-wise control.
    pub async fn goto(&mut self, goal: Goal) -> Result<(), ActionError> {
        self.goto_with(goal, GotoOpts::default()).await
    }

    pub async fn goto_with(&mut self, goal: Goal, opts: GotoOpts) -> Result<(), ActionError> {
        let mut nav = Navigator::new(goal, opts);
        while nav.tick(self).await? != NavStatus::Arrived {}
        Ok(())
    }

    /// Releases the movement controls, e.g. after dropping a `goto` future or a [`Navigator`].
    pub fn stop_pathing(&mut self) {
        if let Some(c) = self.controls() {
            c.stop();
        }
    }

    /// Feet block position of a physics bot (for building goals).
    pub fn block_position(&self) -> Option<BlockPos> {
        let p = self.movement.as_ref()?.position()?;
        Some([p[0].floor() as i32, (p[1] + 0.01).floor() as i32, p[2].floor() as i32])
    }
}
