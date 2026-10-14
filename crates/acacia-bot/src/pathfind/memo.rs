//! [`Terrain`](super::Terrain)'s per-position memo of cells: a direct-mapped front for the physics' repeated
//! lookups, the map behind it, block physics per cell kind, and recording which cells a computation read
//! (the rollout cache's validity check, `cache.rs`).

use std::cell::{Cell as Shared, RefCell};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

use acacia_physics::{BlockPhysics, BlockPos};

use super::cell::{Cell, UNLOADED};
use super::terrain::{PosHasher, PosMap};

/// Front slots (a power of two).
const FRONT: usize = 2048;
const NOWHERE: BlockPos = [i32::MIN; 3];
const BLANK: Cell = Cell { boxes: &[], flags: UNLOADED, block: u32::MAX, liquid: u32::MAX };

/// One memoized cell and the recording epoch that last saw it read.
#[derive(Clone, Copy)]
struct Slot {
    pos: BlockPos,
    cell: Cell,
    seen: u32,
}

pub(crate) struct CellMemo {
    front: RefCell<Box<[Slot; FRONT]>>,
    map: RefCell<PosMap<(Cell, u32)>>,
    physics: RefCell<HashMap<u64, BlockPhysics, BuildHasherDefault<PosHasher>>>,
    /// The running recording's epoch (0: none), recordings so far, and the cells read, each once.
    epoch: Shared<u32>,
    recordings: Shared<u32>,
    reads: RefCell<Vec<(BlockPos, u64)>>,
}

impl Default for CellMemo {
    fn default() -> Self {
        let front = RefCell::new(Box::new([Slot { pos: NOWHERE, cell: BLANK, seen: 0 }; FRONT]));
        Self { front, map: RefCell::default(), physics: RefCell::default(), epoch: Shared::new(0), recordings: Shared::new(0), reads: RefCell::default() }
    }
}

fn slot(p: BlockPos) -> usize {
    let h = (p[0] as u32).wrapping_mul(0x9e37_79b1) ^ (p[1] as u32).wrapping_mul(0x85eb_ca77) ^ (p[2] as u32).wrapping_mul(0xc2b2_ae3d);
    (h >> 7) as usize & (FRONT - 1)
}

impl CellMemo {
    /// The cell at `p`, classified by `classify` the first time.
    pub fn get(&self, p: BlockPos, classify: impl FnOnce() -> Cell) -> Cell {
        let epoch = self.epoch.get();
        let i = slot(p);
        {
            let front = self.front.borrow();
            // While recording, a slot not yet seen by this recording goes the slow way to be logged.
            if front[i].pos == p && (epoch == 0 || front[i].seen == epoch) {
                return front[i].cell;
            }
        }
        let mut map = self.map.borrow_mut();
        let e = map.entry(p).or_insert_with(|| (classify(), 0));
        if epoch != 0 && e.1 != epoch {
            e.1 = epoch;
            self.reads.borrow_mut().push((p, e.0.id()));
        }
        self.front.borrow_mut()[i] = Slot { pos: p, cell: e.0, seen: e.1 };
        e.0
    }

    /// The block physics of cell `c`, computed by `compute` once per block and liquid pair.
    pub fn physics(&self, c: Cell, compute: impl FnOnce() -> BlockPhysics) -> BlockPhysics {
        *self.physics.borrow_mut().entry(c.id()).or_insert_with(compute)
    }

    pub fn recording(&self) -> bool {
        self.epoch.get() != 0
    }

    /// Runs `f`, returning what it returned and every cell read through [`CellMemo::get`] meanwhile, with
    /// its [`Cell::id`]. Not reentrant.
    pub fn record<R>(&self, f: impl FnOnce() -> R) -> (R, Vec<(BlockPos, u64)>) {
        debug_assert!(!self.recording(), "nested recording");
        self.recordings.set(self.recordings.get() + 1);
        self.epoch.set(self.recordings.get());
        let r = f();
        self.epoch.set(0);
        (r, std::mem::take(&mut *self.reads.borrow_mut()))
    }
}
