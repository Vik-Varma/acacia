//! Previous-path favouring for re-plans (Baritone's backtrack cost favouring); bounds in docs/pathfind-plan.md
//! "C1 design".

use std::collections::HashSet;
use std::hash::BuildHasherDefault;

use acacia_physics::BlockPos;

use super::goal::{Goal, Speed};
use super::terrain::PosHasher;

/// Share of an edge's cost above its heuristic floor that is kept on a favoured cell.
const COEFFICIENT: f32 = 0.5;

/// Cells of a path, made cheaper to step onto.
#[derive(Debug, Clone, Default)]
pub struct Favour {
    cells: HashSet<BlockPos, BuildHasherDefault<PosHasher>>,
}

impl Favour {
    pub fn new(cells: impl IntoIterator<Item = BlockPos>) -> Self {
        Self { cells: cells.into_iter().collect() }
    }

    pub fn contains(&self, p: BlockPos) -> bool {
        self.cells.contains(&p)
    }

    /// The edge `from`→`to` costing `cost`, discounted onto a favoured cell, never below the heuristic's
    /// estimate between its ends at `speed`.
    pub(crate) fn cost(&self, from: BlockPos, to: BlockPos, cost: f32, speed: Speed) -> f32 {
        if !self.contains(to) {
            return cost;
        }
        let floor = Goal::Block(to).estimate(from, speed, false).min(cost);
        floor + COEFFICIENT * (cost - floor)
    }
}
