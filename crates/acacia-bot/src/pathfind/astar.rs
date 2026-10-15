//! A* over feet block positions with Baritone's best-partial-path fallback (azalea `astar/mod.rs`).

use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::collections::hash_map::Entry as Slot;
use std::time::Instant;

use acacia_physics::{BlockPos, PlayerState, Vec3};

use super::mac::Macro;
use super::exit::has_exit;
use super::goal::Goal;
use super::cache::RolloutCache;
use super::macros::{MACRO_TIME_SHARE, MacroStats, Macros};
use super::moves::{self, Ctx, Edge, MoveKind};
use super::node::{Entry, KeyMap, Momentum, NodeKey, run_entry};
pub use super::opts::{Path, SearchOpts};
use super::terrain::{Blocks, EPS, Spot, Terrain};
use super::work::Work;
use super::PathNode;

/// Partial-path candidates are the best nodes by `h + g / c` for each coefficient (Baritone).
const COEFFICIENTS: [f32; 7] = [1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 15.0];
const MIN_IMPROVEMENT: f32 = 0.01;
/// Slower than this (blocks per tick) on the ground counts as standing.
const AT_REST: f32 = 0.05;

#[derive(Clone, Copy)]
struct Rec {
    key: NodeKey,
    spot: Spot,
    kind: MoveKind,
    parent: u32,
    g: f32,
    /// `g` without [`SearchOpts::favour`]'s discounts.
    raw: f32,
    /// Scaffold blocks the path to here places.
    placed: u32,
    /// Index into the search's work list, `NONE` for plain moves.
    work: u32,
    /// Index into the search's macro list, `NONE` for classic moves.
    mac: u32,
    mom: Momentum,
}

const NONE: u32 = u32::MAX;

struct Open {
    f: f32,
    g: f32,
    index: u32,
}

impl PartialEq for Open {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Open {
    // Min-heap on f; ties go to the deeper node.
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.total_cmp(&self.f).then(self.g.total_cmp(&other.g))
    }
}

/// Node whose spot contains the feet at `feet` (the player may be off-centre, falling or swimming):
/// the feet cell itself, else the nearest valid neighbour cell.
pub fn start_node<B: Blocks>(t: &Terrain<B>, feet: Vec3) -> Option<(BlockPos, Spot)> {
    let [x, z] = [feet[0].floor() as i32, feet[2].floor() as i32];
    let y = (feet[1] - 0.5 - EPS).ceil() as i32;
    if let Some(s) = t.spot([x, y, z]) {
        return Some(([x, y, z], s));
    }
    let mut best: Option<(f32, BlockPos, Spot)> = None;
    for (dx, dy, dz) in (-1..=1).flat_map(|dx| (-2..=1).flat_map(move |dy| (-1..=1).map(move |dz| (dx, dy, dz)))) {
        let p = [x + dx, y + dy, z + dz];
        let Some(s) = t.spot(p) else { continue };
        let d = [p[0] as f32 + 0.5 - feet[0], s.feet - feet[1], p[2] as f32 + 0.5 - feet[2]];
        let d = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        if best.is_none_or(|(b, ..)| d < b) {
            best = Some((d, p, s));
        }
    }
    best.map(|(_, p, s)| (p, s))
}

/// Searches from a player standing at `feet`.
pub fn search<B: Blocks>(t: &Terrain<B>, feet: Vec3, goal: &Goal, opts: &SearchOpts) -> Path {
    let mut st = PlayerState::new(feet);
    st.on_ground = true;
    search_from(t, &st, goal, opts)
}

/// Searches from the player's live state: its momentum is the start node's, and its attributes and
/// effects shape every macro move.
pub fn search_from<B: Blocks>(t: &Terrain<B>, live: &PlayerState, goal: &Goal, opts: &SearchOpts) -> Path {
    search_cached(t, live, goal, opts, &RolloutCache::default())
}

/// [`search_from`] reusing (and adding to) the macro-move results in `cache`; they are dropped first if
/// they were found for another profile or player.
pub fn search_cached<B: Blocks>(t: &Terrain<B>, live: &PlayerState, goal: &Goal, opts: &SearchOpts, cache: &RolloutCache) -> Path {
    let started = Instant::now();
    let empty = |complete| Path { nodes: Vec::new(), complete, unreachable: false, cost: 0.0, visited: 0, macro_stats: MacroStats::default() };
    let Some((start, spot)) = start_node(t, live.pos) else { return empty(false) };
    if goal.success(start) {
        return empty(true);
    }
    let entry = match live.on_ground {
        false => Entry::Air,
        true if live.vel[0].hypot(live.vel[2]) < AT_REST => Entry::Rest,
        true => run_entry(live.vel),
    };
    let start = NodeKey { entry, ..NodeKey::rest(start) };
    let mom = Momentum::snap(live, start.pos);
    let mut recs = vec![Rec { key: start, spot, kind: MoveKind::Walk, parent: NONE, g: 0.0, raw: 0.0, placed: 0, work: NONE, mac: NONE, mom }];
    let (mut works, mut macs): (Vec<Work>, Vec<Macro>) = (Vec::new(), Vec::new());
    let mut index = KeyMap::default();
    index.insert(start, 0u32);
    let mut open = BinaryHeap::from([Open { f: 0.0, g: 0.0, index: 0 }]);
    let speed = opts.speed();
    let mut best = [0u32; 7];
    let mut best_score = [goal.estimate(start.pos, speed, entry == Entry::Rest); 7];
    let mut edges: Vec<Edge> = Vec::with_capacity(32);
    let mut visited = 0;
    let macros = opts.macros.then(|| Macros {
        profile: opts.profile,
        leaps: opts.parkour,
        hops: opts.hops,
        base: live,
        cache: cache.claim(opts.profile, live),
        budget: opts.max_rollout_ticks,
        deadline: started + opts.budget / MACRO_TIME_SHARE,
        stats: Cell::default(),
    });
    let mut ctx = Ctx { parkour: opts.parkour, dig: opts.allow_dig, bridge: opts.allow_bridge, doors: opts.open_doors, kit: &opts.kit, blocks_left: 0, macros };
    let stats = |ctx: &Ctx| ctx.macros.as_ref().map_or_else(MacroStats::default, |m| m.stats.get());
    let mut cut = false;

    while let Some(Open { g, index: i, .. }) = open.pop() {
        let rec = recs[i as usize];
        if g > rec.g {
            continue;
        }
        if goal.success(rec.key.pos) {
            let nodes = reconstruct(&recs, &works, &macs, i);
            return Path { nodes, complete: true, unreachable: false, cost: rec.raw, visited, macro_stats: stats(&ctx) };
        }
        visited += 1;
        // Checked often: a node with macro moves costs many rollouts.
        if visited >= opts.max_nodes || (visited % 8 == 0 && started.elapsed() >= opts.budget) {
            cut = true;
            break;
        }
        edges.clear();
        ctx.blocks_left = opts.kit.blocks.saturating_sub(rec.placed);
        moves::successors(t, rec.key.pos, rec.spot, (rec.key.entry, &rec.mom), &ctx, &mut edges);
        for e in &edges {
            let g2 = g + opts.favour.as_ref().map_or(e.cost, |f| f.cost(rec.key.pos, e.pos, e.cost, speed));
            let key = NodeKey { pos: e.pos, entry: e.entry, ..rec.key };
            let (placed, raw) = (rec.placed + e.work.places(), rec.raw + e.cost);
            let mut next = Rec { key, spot: e.spot, kind: e.kind, parent: i, g: g2, raw, placed, work: NONE, mac: NONE, mom: e.mom };
            let j = match index.entry(key) {
                Slot::Occupied(o) => {
                    let j = *o.get();
                    if recs[j as usize].g <= g2 {
                        continue;
                    }
                    j
                }
                Slot::Vacant(v) => {
                    let j = recs.len() as u32;
                    recs.push(next);
                    v.insert(j);
                    j
                }
            };
            if !e.work.is_empty() {
                next.work = works.len() as u32;
                works.push(e.work);
            }
            if let Some(m) = e.mac {
                next.mac = macs.len() as u32;
                macs.push(m);
            }
            recs[j as usize] = next;
            let h = goal.estimate(e.pos, speed, e.entry == Entry::Rest);
            open.push(Open { f: g2 + h, g: g2, index: j });
            for (k, c) in COEFFICIENTS.iter().enumerate() {
                let score = h + g2 / c;
                if best_score[k] - score > MIN_IMPROVEMENT {
                    (best[k], best_score[k]) = (j, score);
                }
            }
        }
    }
    let macro_stats = stats(&ctx);
    let unreachable = !cut && !macro_stats.exhausted && !t.saw_unloaded();
    let mut pick = best.iter().copied().find(|&b| b != 0).unwrap_or(0);
    // Back off a dead end to the last node on the way that still leads somewhere.
    while pick != 0 && !has_exit(t, recs[pick as usize].key.pos, recs[pick as usize].spot, opts, recs[pick as usize].placed) {
        pick = recs[pick as usize].parent;
    }
    let cost = recs[pick as usize].raw;
    Path { nodes: reconstruct(&recs, &works, &macs, pick), complete: false, unreachable, cost, visited, macro_stats }
}

fn reconstruct(recs: &[Rec], works: &[Work], macs: &[Macro], mut i: u32) -> Vec<PathNode> {
    let mut nodes = Vec::new();
    while let Some(r) = recs.get(i as usize).filter(|r| r.parent != NONE) {
        let work = works.get(r.work as usize).copied().unwrap_or_default();
        let mac = macs.get(r.mac as usize).copied();
        let cost = r.raw - recs[r.parent as usize].raw;
        nodes.push(PathNode { pos: r.key.pos, feet: r.spot.feet, kind: r.kind, wet: r.spot.wet, work, entry: r.key.entry, mom: r.mom, mac, cost });
        i = r.parent;
    }
    nodes.reverse();
    nodes
}
