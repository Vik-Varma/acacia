//! Which leaps a node offers: landing cells around it (cardinal up to 7 blocks, the diagonals (2,1) (2,2)
//! (3,1) (3,2) (4,1)), each over at least one gap cell, through a corridor that is clear at jump height, onto
//! a dry floor one up to three down or into water deeper below.

use std::sync::LazyLock;

use acacia_physics::BlockPos;

use super::cell::Rect;
use super::moves::offset;
use super::solve::{STRAIGHT, Shape, Solved, Solver, WIDE};
use super::terrain::{Blocks, Spot, SpotKind, Terrain};
use crate::movement::Rollout;

/// Farthest cardinal leap (blocks to the landing cell): from rest a 4-gap needs a two-block run-up.
const MAX_CARDINAL: i32 = 7;
const DIAGONALS: [(i32, i32); 5] = [(2, 1), (2, 2), (3, 1), (3, 2), (4, 1)];
/// Dry landings from one up to three down.
const DRY_DY: [i32; 5] = [1, 0, -1, -2, -3];
/// Deepest drop scanned for a water landing.
const MAX_WATER_DROP: i32 = 16;

/// A landing column relative to the node and the cells the straight line to it crosses.
struct Offset {
    to: (i32, i32),
    crossed: Vec<(i32, i32)>,
    diagonal: bool,
}

/// Every offset, nearest first, so a blocked cell is found before the targets it shadows.
static OFFSETS: LazyLock<Vec<Offset>> = LazyLock::new(|| {
    let mut tos: Vec<(i32, i32)> = (2..=MAX_CARDINAL).map(|k| (k, 0)).chain(DIAGONALS).collect();
    tos = tos.iter().flat_map(|&(a, b)| [(a, b), (b, a)]).flat_map(|(a, b)| [(a, b), (-a, b), (a, -b), (-a, -b)]).collect();
    tos.sort_by_key(|&(a, b)| (a * a + b * b, a, b));
    tos.dedup();
    tos.into_iter().map(|to| Offset { to, crossed: crossed(to), diagonal: to.0 != 0 && to.1 != 0 }).collect()
});

/// Cells (besides both ends) the segment between the two cell centres passes through.
fn crossed((a, b): (i32, i32)) -> Vec<(i32, i32)> {
    let steps = 16 * (a.abs() + b.abs());
    let mut out: Vec<(i32, i32)> = Vec::new();
    for i in 1..steps {
        let f = i as f32 / steps as f32;
        let c = ((0.5 + a as f32 * f).floor() as i32, (0.5 + b as f32 * f).floor() as i32);
        if c != (0, 0) && c != (a, b) && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Where a leap into column `c` can land: dry floors near the start's height, else the first water below.
fn landings<B: Blocks>(t: &Terrain<B>, c: BlockPos) -> Vec<(BlockPos, Spot)> {
    let dry = DRY_DY.iter().map(|&dy| offset(c, 0, dy, 0)).filter_map(|q| Some((q, t.spot(q).filter(|s| s.kind == SpotKind::Stand && !s.wet)?)));
    let mut out: Vec<_> = dry.collect();
    for dy in 1..=MAX_WATER_DROP {
        let q = offset(c, 0, -dy, 0);
        if let Some(s) = t.spot(q) {
            if s.wet && dy > 1 {
                out.push((q, s));
            }
            break;
        }
        if !t.passable(q) {
            break;
        }
    }
    out
}

/// The leaps from `p` (standing at `from`), solved from `start`.
pub(crate) fn leaps<B: Blocks>(t: &Terrain<B>, p: BlockPos, from: Spot, start: &Rollout, solver: &mut Solver<Terrain<B>>) -> Vec<(BlockPos, Spot, Solved)> {
    let mut found = Vec::new();
    // Per crossed cell and lane shape: (body clear at takeoff and jump height, no floor at the start's level).
    let mut cells: Vec<((i32, i32, bool), bool, bool)> = Vec::new();
    let mut probe = |(dx, dz): (i32, i32), diagonal: bool| {
        if let Some(&(_, clear, gap)) = cells.iter().find(|e| e.0 == (dx, dz, diagonal)) {
            return (clear, gap);
        }
        let q = offset(p, dx, 0, dz);
        // Cardinal lanes are crossed at the body's width, diagonal ones anywhere in the cell.
        let rect = if diagonal { Rect::lane(1, 1) } else { Rect::lane(dx.signum(), dz.signum()) };
        let clear = t.body_clear(q, rect, from.feet) && t.body_clear(q, rect, from.feet + 1.0);
        let gap = t.spot(q).is_none() && t.spot(offset(q, 0, 1, 0)).is_none();
        cells.push(((dx, dz, diagonal), clear, gap));
        (clear, gap)
    };
    for off in OFFSETS.iter() {
        let mut gap = false;
        let clear = off.crossed.iter().all(|&c| {
            let (clear, g) = probe(c, off.diagonal);
            gap |= g;
            clear
        });
        if !clear || !gap {
            continue;
        }
        let start = facing(start, off.to.0 as f32, off.to.1 as f32);
        for (q, to) in landings(t, offset(p, off.to.0, 0, off.to.1)) {
            let straight = Shape { drop: to.wet || q[1] < p[1], ..STRAIGHT };
            let wide = Shape { drop: straight.drop, ..WIDE };
            let solved = solver.solve(&start, q, to.feet, to.wet, straight);
            if let Some(s) = solved.or_else(|| off.diagonal.then(|| solver.solve(&start, q, to.feet, to.wet, wide)).flatten()) {
                found.push((q, to, s));
            }
        }
    }
    found
}

/// `start` turned towards `(dx, dz)` if it stands still: a standing player faces where it goes next (a `Human`
/// aim eases from the facing, so it shapes the leap).
fn facing(start: &Rollout, dx: f32, dz: f32) -> Rollout {
    let mut r = start.clone();
    if r.st.vel[0] == 0.0 && r.st.vel[2] == 0.0 {
        r.st.yaw = (-dx).atan2(dz).to_degrees();
    }
    r
}
