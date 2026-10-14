use std::f32::consts::SQRT_2;

use acacia_physics::BlockPos;

use super::costs::{self, COST_HEURISTIC, JUMP_PENALTY, SPRINT, WALK};

/// Where to go, in feet block positions.
#[derive(Debug, Clone, PartialEq)]
pub enum Goal {
    Block(BlockPos),
    /// Within `radius` blocks (Euclidean) of the position.
    Near(BlockPos, f32),
    XZ(i32, i32),
    Y(i32),
    /// Any of the goals.
    Or(Vec<Goal>),
}

/// How a goal turns distance into ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Speed {
    /// Ticks per horizontal block.
    pub per_block: f32,
    /// Height is gained or lost during the horizontal run (macro moves), not after it: the larger term counts.
    pub overlap: bool,
}

impl Speed {
    /// Classic moves: sprinting, then climbing or falling (azalea's heuristics, which Baritone also uses).
    pub const CLASSIC: Speed = Speed { per_block: COST_HEURISTIC, overlap: false };

    fn combine(&self, xz: f32, y: f32) -> f32 {
        if self.overlap { xz.max(y) } else { xz + y }
    }
}

impl Goal {
    pub fn success(&self, p: BlockPos) -> bool {
        match self {
            Goal::Block(g) => p == *g,
            Goal::Near(g, r) => dist_sq(p, *g) <= r * r,
            Goal::XZ(x, z) => p[0] == *x && p[2] == *z,
            Goal::Y(y) => p[1] == *y,
            Goal::Or(goals) => goals.iter().any(|g| g.success(p)),
        }
    }

    /// Estimated ticks to the goal for classic moves.
    pub fn heuristic(&self, p: BlockPos) -> f32 {
        self.estimate(p, Speed::CLASSIC)
    }

    pub(crate) fn estimate(&self, p: BlockPos, s: Speed) -> f32 {
        match self {
            Goal::Block(g) => s.combine(xz((g[0] - p[0]) as f32, (g[2] - p[2]) as f32) * s.per_block, y((g[1] - p[1]) as f32)),
            Goal::Near(g, r) => {
                let d = dist_sq(p, *g).sqrt();
                let exact = Goal::Block(*g).estimate(p, s);
                if d <= *r { 0.0 } else { exact * (1.0 - r / d) }
            }
            Goal::XZ(x, z) => xz((x - p[0]) as f32, (z - p[2]) as f32) * s.per_block,
            Goal::Y(gy) => y((gy - p[1]) as f32),
            Goal::Or(goals) => goals.iter().map(|g| g.estimate(p, s)).fold(f32::INFINITY, f32::min),
        }
    }
}

fn dist_sq(a: BlockPos, b: BlockPos) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]].map(|v| v as f32);
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// Octile distance (blocks).
fn xz(dx: f32, dz: f32) -> f32 {
    let (x, z) = (dx.abs(), dz.abs());
    let (diagonal, straight) = if x < z { (x, z - x) } else { (z, x - z) };
    diagonal * SQRT_2 + straight
}

fn y(dy: f32) -> f32 {
    if dy > 0.0 {
        // Jumping forward is the usual way up; the xz term already counted one sprint block.
        (costs::jump_one_block().max(WALK) + JUMP_PENALTY - SPRINT) * dy
    } else {
        costs::fall_ticks(2.0) / 2.0 * -dy
    }
}
