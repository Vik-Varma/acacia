use std::sync::{Arc, Mutex};

use acacia_physics::BlockPos;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Surveying,
    Building,
    Restocking,
    /// Removing scaffold once every cell is right.
    Cleaning,
    /// Nothing doable: missing materials no station has, or unreachable cells.
    Waiting,
    Done,
}

/// A builder's progress, readable while it runs through [`StatusHandle`].
#[derive(Debug, Clone, Default)]
pub struct BuildStatus {
    pub phase: Phase,
    /// Footprint cells with a target (structure void excluded).
    pub cells: usize,
    pub done: usize,
    pub to_place: usize,
    pub to_break: usize,
    /// Not loaded yet.
    pub unknown: usize,
    /// Fill-only jobs: cells holding another block, left alone.
    pub kept: usize,
    /// Targets outside the builder's block set, skipped.
    pub unsupported: Vec<(BlockPos, String)>,
    /// Cells given up on after repeated failures.
    pub failed: Vec<BlockPos>,
    /// Temporary blocks still standing.
    pub scaffold: usize,
    /// Seals left in place outside the footprint, holding liquids back.
    pub seals: Vec<BlockPos>,
    /// Items still needed that neither the inventory nor any station has.
    pub missing: Vec<String>,
    pub placed: u32,
    pub broken: u32,
    pub restocks: u32,
}

pub type StatusHandle = Arc<Mutex<BuildStatus>>;
