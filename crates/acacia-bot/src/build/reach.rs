//! Where to stand and what to click: a placement's state depends on the clicked face, the click height and the
//! player's facing (see `rules`), so a cell is placed from a spot whose eye makes the click give the target.

use acacia_physics::{BlockPos, Vec3};

use super::rules;
use super::structure::Block;
use crate::interact::{Face, BLOCK_REACH};
use crate::movement::EYE_HEIGHT;
use crate::pathfind::process::{BlockView, Look};

/// Margin under the server's reach.
const MARGIN: f32 = 0.35;
/// More margin when choosing a stand: the pilot stops up to its tolerance off the cell's centre.
pub const STAND_SLACK: f32 = 0.45;
/// How far around a cell stands are looked for.
const RADIUS: i32 = 4;
/// Pillar stands (over air) up to this many blocks above the ground under them.
const MAX_PILLAR: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Click {
    pub against: BlockPos,
    pub face: Face,
    /// Relative to `against`'s minimum corner.
    pub click: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stand {
    /// Feet cell.
    pub feet: BlockPos,
    /// Blocks of pillar the pilot has to build under it (0: stands on something).
    pub pillar: i32,
}

pub fn eye_at(feet: BlockPos) -> Vec3 {
    [feet[0] as f32 + 0.5, feet[1] as f32 + EYE_HEIGHT, feet[2] as f32 + 0.5]
}

fn dist_to_block(eye: Vec3, p: BlockPos) -> f32 {
    let d = |e: f32, lo: i32| (lo as f32 - e).max(e - (lo + 1) as f32).max(0.0);
    let (x, y, z) = (d(eye[0], p[0]), d(eye[1], p[1]), d(eye[2], p[2]));
    (x * x + y * y + z * z).sqrt()
}

pub fn in_reach(eye: Vec3, p: BlockPos) -> bool {
    within(eye, p, 0.0)
}

pub fn within(eye: Vec3, p: BlockPos, slack: f32) -> bool {
    dist_to_block(eye, p) <= BLOCK_REACH - MARGIN - slack
}

/// The eye is on the outer side of `face` of the block at `p`: the face can be seen.
fn sees(eye: Vec3, p: BlockPos, face: Face) -> bool {
    let lo = |i: usize| p[i] as f32;
    match face {
        Face::Up => eye[1] > lo(1) + 1.0,
        Face::Down => eye[1] < lo(1),
        Face::North => eye[2] < lo(2),
        Face::South => eye[2] > lo(2) + 1.0,
        Face::West => eye[0] < lo(0),
        Face::East => eye[0] > lo(0) + 1.0,
    }
}

/// Clicks on `face` of `against`: its centre, or low and high on a side face (a slab's or stair's half follows the
/// click). Only points on the block's shape: a full cube anywhere, a half slab its own half of the side faces.
fn points(face: Face, against: &BlockView) -> Vec<Vec3> {
    let [ox, oy, oz] = face.offset().map(|v| v as f32);
    let c = [0.5 + 0.5 * ox, 0.5 + 0.5 * oy, 0.5 + 0.5 * oz];
    let side = |ys: &[f32]| ys.iter().map(|&y| [c[0], y, c[2]]).collect();
    let slab = against.name.ends_with("_slab").then(|| against.prop("minecraft:vertical_half")).flatten();
    match (face, slab) {
        (_, None) if !against.full => Vec::new(),
        (Face::Up | Face::Down, None) => vec![c],
        (_, None) => side(&[0.25, 0.75]),
        (Face::Up | Face::Down, _) => Vec::new(),
        (_, Some("top")) => side(&[0.75]),
        (_, Some(_)) => side(&[0.25]),
    }
}

/// A click from `eye` that places `target` into `pos` with its state, reaching `slack` short of the limit.
pub fn click_for(pos: BlockPos, target: &Block, eye: Vec3, look: &dyn Look, slack: f32) -> Option<Click> {
    for face in Face::ALL {
        let o = face.offset();
        let against = [pos[0] - o[0], pos[1] - o[1], pos[2] - o[2]];
        let Some(b) = look.block(against) else { continue };
        if !b.solid() || b.interactive || !sees(eye, against, face) || !within(eye, against, slack) {
            continue;
        }
        let Some(click) = points(face, &b).into_iter().find(|c| rules::gives(target, face, *c, eye, pos, &b)) else { continue };
        return Some(Click { against, face, click });
    }
    None
}

fn passable(look: &dyn Look, p: BlockPos) -> bool {
    look.block(p).is_some_and(|b| !b.solid())
}

/// The bot's box at `feet` keeps clear of `pos`.
fn clear_of(feet: BlockPos, pos: BlockPos) -> bool {
    feet != pos && [feet[0], feet[1] + 1, feet[2]] != pos
}

/// Up to `limit` spots near `pos` the bot can stand at (or pillar up to) with `ok(eye)`, cheapest from `from` first.
pub fn stands(pos: BlockPos, from: Vec3, look: &dyn Look, avoid: &dyn Fn(BlockPos) -> bool, ok: &dyn Fn(Vec3) -> bool, limit: usize) -> Vec<Stand> {
    let mut all = Vec::new();
    for dx in -RADIUS..=RADIUS {
        for dz in -RADIUS..=RADIUS {
            for dy in -3..=1 {
                let feet = [pos[0] + dx, pos[1] + dy, pos[2] + dz];
                if !clear_of(feet, pos) || avoid(feet) || !within(eye_at(feet), pos, STAND_SLACK) || !passable(look, feet) || !passable(look, [feet[0], feet[1] + 1, feet[2]]) {
                    continue;
                }
                if let Some(pillar) = support(look, feet) {
                    all.push(Stand { feet, pillar });
                }
            }
        }
    }
    let cost = |s: &Stand| {
        let d = [s.feet[0] as f32 + 0.5 - from[0], s.feet[1] as f32 - from[1], s.feet[2] as f32 + 0.5 - from[2]];
        (d[0] * d[0] + d[2] * d[2]).sqrt() + d[1].abs() * 2.0 + s.pillar as f32 * 4.0
    };
    all.sort_by(|a, b| cost(a).total_cmp(&cost(b)));
    all.into_iter().filter(|s| ok(eye_at(s.feet))).take(limit).collect()
}

/// Blocks of pillar under `feet` down to something solid; `None` past [`MAX_PILLAR`] or over liquid or unloaded cells.
fn support(look: &dyn Look, feet: BlockPos) -> Option<i32> {
    for n in 0..=MAX_PILLAR {
        let b = look.block([feet[0], feet[1] - 1 - n, feet[2]])?;
        if b.liquid {
            return None;
        }
        if b.solid() {
            return Some(n);
        }
    }
    None
}
