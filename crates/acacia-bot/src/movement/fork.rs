//! A copy of the simulation that steps ahead without touching the live one, for planning and prediction.

use acacia_physics::{self as physics, PlayerState, TickOutput, WorldView};

use super::keys::KeyLatch;
use super::{Controls, Movement};

/// The simulation and key latches only: no server history, teleports or packets.
#[derive(Debug, Clone)]
pub struct Rollout {
    pub st: PlayerState,
    pub(crate) keys: KeyLatch,
    pub using_item: bool,
}

impl Rollout {
    /// A rollout from `st` with no keys held before.
    pub fn new(st: PlayerState) -> Self {
        Self { st, keys: KeyLatch::default(), using_item: false }
    }

    /// Simulates one tick with `c` held, as [`Movement::tick`] does.
    pub fn step(&mut self, c: &Controls, world: &(impl WorldView + ?Sized)) -> TickOutput {
        let input = self.keys.advance(c, &self.st, world, self.using_item, c.sneak);
        physics::tick(&mut self.st, &input, world)
    }
}

impl Movement {
    /// The simulation as of now, to step ahead of the live one (`None` before movement starts).
    pub fn fork(&self) -> Option<Rollout> {
        let mut st = self.physics.clone()?;
        st.equipment = self.equipment;
        st.equipment.elytra = self.elytra;
        Some(Rollout { st, keys: self.keys.clone(), using_item: self.using_item })
    }

    /// The live simulation state (`None` before movement starts).
    pub fn state(&self) -> Option<&PlayerState> {
        self.physics.as_ref()
    }
}
