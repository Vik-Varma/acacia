//! Macro-move results kept across nodes, searches and re-plans: a result is stored relative to its node
//! with every cell its rollouts read, and reused at any node whose momentum and family match and whose
//! cells read the same (failures too). Positions are translated in f32, so a reused landing may sit a
//! float rounding off the one a fresh rollout would give, far under `Momentum`'s quantum.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash};

use acacia_physics::{BlockPos, PlayerState};

use super::control::RunCtl;
use super::hop::Ran;
use super::leap::LeapCtl;
use super::node::Momentum;
use super::profile::MovementProfile;
use super::solve::{Miss, Solved};
use super::terrain::{Blocks, PosHasher, Spot, Terrain};

/// Results kept per key; more means the same move from the same momentum met that many surroundings.
const PER_KEY: usize = 32;
/// Results a cache keeps across searches before starting over (each holds its read cells, a few KB).
const MAX_RESULTS: usize = 50_000;

/// Moved to another node `d` away.
pub(crate) trait Shift {
    fn shift(&mut self, d: BlockPos);
}

fn add(p: BlockPos, d: BlockPos) -> BlockPos {
    [p[0] + d[0], p[1] + d[1], p[2] + d[2]]
}

impl Shift for BlockPos {
    fn shift(&mut self, d: BlockPos) {
        *self = add(*self, d);
    }
}

impl Shift for Spot {
    fn shift(&mut self, d: BlockPos) {
        self.feet += d[1] as f32;
    }
}

/// Only the position: a stored end state is read for its position, velocity and flags.
impl Shift for PlayerState {
    fn shift(&mut self, d: BlockPos) {
        (0..3).for_each(|i| self.pos[i] += d[i] as f32);
    }
}

impl Shift for LeapCtl {
    fn shift(&mut self, d: BlockPos) {
        self.target.shift(d);
        self.feet += d[1] as f32;
    }
}

impl Shift for RunCtl {
    fn shift(&mut self, d: BlockPos) {
        self.target.shift(d);
        self.feet += d[1] as f32;
    }
}

impl Shift for Solved {
    fn shift(&mut self, d: BlockPos) {
        self.ctl.shift(d);
        self.end.shift(d);
    }
}

impl Shift for Ran {
    fn shift(&mut self, d: BlockPos) {
        self.ctl.shift(d);
        self.end.shift(d);
    }
}

impl<T: Shift> Shift for Option<T> {
    fn shift(&mut self, d: BlockPos) {
        if let Some(v) = self {
            v.shift(d);
        }
    }
}

impl<T: Shift> Shift for Result<T, Miss> {
    fn shift(&mut self, d: BlockPos) {
        if let Ok(v) = self {
            v.shift(d);
        }
    }
}

impl<A: Shift, B: Shift, C: Shift> Shift for (A, B, C) {
    fn shift(&mut self, d: BlockPos) {
        self.0.shift(d);
        self.1.shift(d);
        self.2.shift(d);
    }
}

struct Entry<V> {
    /// Cells read, relative to the node, with their ids.
    reads: Box<[(BlockPos, u64)]>,
    value: V,
}

/// One move family's results.
pub(crate) struct Memo<K, V> {
    map: HashMap<K, Vec<Entry<V>>, BuildHasherDefault<PosHasher>>,
}

impl<K, V> Default for Memo<K, V> {
    fn default() -> Self {
        Self { map: HashMap::default() }
    }
}

impl<K, V> Memo<K, V> {
    fn len(&self) -> usize {
        self.map.values().map(Vec::len).sum()
    }
}

impl<K: Hash + Eq, V: Shift + Clone> Memo<K, V> {
    /// The result for `key` at node `p`: a stored one whose cells all read the same here, else `compute`'s.
    pub fn get_or<B: Blocks>(&mut self, t: &Terrain<B>, p: BlockPos, key: K, n: &mut Counts, compute: impl FnOnce() -> V) -> V {
        if n.off {
            n.misses += 1;
            return compute();
        }
        let entries = self.map.entry(key).or_default();
        for i in 0..entries.len() {
            let reads = &mut entries[i].reads;
            match reads.iter().position(|&(q, id)| t.cell(add(p, q)).id() != id) {
                // The cell that told this entry apart goes first: neighbouring nodes tend to differ there too.
                Some(j) => reads.swap(0, j),
                None => {
                    n.hits += 1;
                    entries.swap(0, i);
                    let mut v = entries[0].value.clone();
                    v.shift(p);
                    return v;
                }
            }
        }
        n.misses += 1;
        let (v, reads) = t.record(compute);
        let mut stored = v.clone();
        stored.shift(p.map(|c| -c));
        let reads = reads.into_iter().map(|(q, id)| (add(q, p.map(|c| -c)), id)).collect();
        if entries.len() == PER_KEY {
            entries.remove(0);
        }
        entries.push(Entry { reads, value: stored });
        v
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub hits: u32,
    pub misses: u32,
    /// Simulate everything and keep nothing (comparisons in tests).
    pub off: bool,
}

/// Leap key beyond the momentum: landing offset, feet height relative to the node (1/1024), into water.
pub(crate) type LeapKey = ((i32, i32, i32), i16, bool);

/// Macro-move results for one movement profile and player (attributes, effects, equipment); a
/// [`Navigator`](super::Navigator) keeps one across its re-plans.
#[derive(Default)]
pub struct RolloutCache {
    /// The player the results hold for: its state at rest at the origin.
    owner: Option<(MovementProfile, PlayerState)>,
    pub(crate) runs: Memo<(Momentum, u8, i32, i16), Option<Ran>>,
    pub(crate) hops: Memo<(Momentum, u8), Option<(BlockPos, Spot, Solved)>>,
    pub(crate) leaps: Memo<(Momentum, LeapKey), Result<Solved, Miss>>,
    pub(crate) counts: Counts,
}

impl RolloutCache {
    /// Empties the cache unless it already holds results for `profile` and the player `base` (and is not full).
    pub(crate) fn claim(&mut self, profile: MovementProfile, base: &PlayerState) {
        let owner = (profile, Momentum::default().synth([0, 0, 0], base));
        let held = self.runs.len() + self.hops.len() + self.leaps.len();
        if self.owner.as_ref() != Some(&owner) || held > MAX_RESULTS {
            let counts = Counts { off: self.counts.off, ..Counts::default() };
            *self = Self { owner: Some(owner), counts, ..Self::default() };
        }
    }

    /// A cache that keeps nothing: every search simulates every move.
    pub fn disabled() -> Self {
        Self { counts: Counts { off: true, ..Counts::default() }, ..Self::default() }
    }
}

/// Feet height relative to node `p` in `Momentum` steps, for keys.
pub(crate) fn feet_key(feet: f32, p: BlockPos) -> i16 {
    ((feet - p[1] as f32) * 1024.0).round() as i16
}
