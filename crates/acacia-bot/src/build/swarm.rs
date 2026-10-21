//! A builder sharing its job through a [`TaskBoard`]: it claims the cells of the step it plans (a stand's batch,
//! or the one cell it acts on), leaves cells other bots claimed, and prefers its home band of the footprint
//! but works anywhere: with exclusive tiles one bot idled while the other finished its own (docs/builder.md "Swarm").

use std::collections::HashSet;
use std::sync::Arc;

use acacia_physics::{BlockPos, Vec3};

use super::board::TaskBoard;
use super::builder::Builder;
use crate::pathfind::process::Ctx;

/// Ticks between heartbeats (the lease must outlast several).
const BEAT: u64 = 20;
/// Stands closer than this (horizontally) to another bot are not used.
const ROOM: f32 = 1.5;
/// Blocks of travel added for work outside the bot's home band: it goes there only when home has nothing
/// near. Bands over repelling from the other bots' feet: repelled, the two bots' areas moved with them and
/// overlapped (live house16: 133 walks of ~1.05 s against one bot's 103 of 0.85 s).
const AWAY: f32 = 10.0;
/// Ticks a cell stays claimed after acting on it: the others see the block only once the server tells them
/// (live: two slabs went into one cell, a double slab).
const GRACE: u64 = 40;

pub(super) struct Swarm {
    board: Arc<dyn TaskBoard>,
    who: String,
    beat: Option<u64>,
    claimed: Vec<BlockPos>,
    /// Cells acted on, with the tick: claimed until [`GRACE`] runs out.
    acted: Vec<(BlockPos, u64)>,
    taken: HashSet<BlockPos>,
    others: Vec<Vec3>,
    /// (rank, bots) the band was cut for, the axis (0 = x, 2 = z) and the band's coordinates on it, end excluded.
    home: ((usize, usize), usize, std::ops::Range<i32>),
}

fn flat(a: Vec3, p: BlockPos) -> f32 {
    (a[0] - (p[0] as f32 + 0.5)).hypot(a[2] - (p[2] as f32 + 0.5))
}

impl Builder {
    /// Shares the job with other builders on `board`, under the name `who` (unique per bot).
    pub fn with_board(mut self, board: Arc<dyn TaskBoard>, who: impl Into<String>) -> Self {
        let who = who.into();
        // On the board from now (feet unknown: NaN crowds nobody), so others count this bot from their first restock.
        board.heartbeat(&who, [f32::NAN; 3]);
        self.swarm = Some(Swarm { board, who, beat: None, claimed: Vec::new(), acted: Vec::new(), taken: HashSet::new(), others: Vec::new(), home: ((0, 1), 0, i32::MIN..i32::MAX) });
        self
    }

    /// No other bot claimed the cell.
    pub(super) fn mine(&self, p: BlockPos) -> bool {
        self.swarm.as_ref().is_none_or(|s| !s.taken.contains(&p))
    }

    /// The cell is beside one another bot claimed (pillars keep out: they would wall off its clicks).
    pub(super) fn theirs(&self, p: BlockPos) -> bool {
        self.swarm.as_ref().is_some_and(|s| s.taken.iter().any(|t| (t[0] - p[0]).abs() <= 1 && (t[2] - p[2]).abs() <= 1 && (t[1] - p[1]).abs() <= 1))
    }

    /// Extra travel, in blocks, for work at `p`: it is outside this bot's home band.
    pub(super) fn away_cost(&self, p: BlockPos) -> f32 {
        match &self.swarm {
            Some(Swarm { home: (_, axis, band), .. }) if !band.contains(&p[*axis]) => AWAY,
            _ => 0.0,
        }
    }

    /// The footprint cut along its longer side into one band per bot, each with as many blocks to place.
    fn cut_home(&mut self, (rank, bots): (usize, usize)) {
        let extent = |i: usize| self.cells.iter().map(|c| c.pos[i]).max().unwrap_or(0) - self.cells.iter().map(|c| c.pos[i]).min().unwrap_or(0);
        let axis = if extent(0) >= extent(2) { 0 } else { 2 };
        let mut at: Vec<i32> = self.cells.iter().filter(|c| !c.target.is_air()).map(|c| c.pos[axis]).collect();
        at.sort_unstable();
        let cut = |k: usize| if k == 0 { i32::MIN } else if k >= bots { i32::MAX } else { at.get(k * at.len() / bots).copied().unwrap_or(i32::MAX) };
        let band = cut(rank)..cut(rank + 1);
        if let Some(s) = &mut self.swarm {
            s.home = ((rank, bots), axis, band);
        }
    }

    /// Feet in `cell` would crowd another bot.
    pub(super) fn crowded(&self, cell: BlockPos) -> bool {
        self.swarm.as_ref().is_some_and(|s| s.others.iter().any(|o| flat(*o, cell) < ROOM && (o[1] - cell[1] as f32).abs() < 2.0))
    }

    /// Another bot's body is in the cell.
    pub(super) fn occupied(&self, p: BlockPos) -> bool {
        let Some(s) = &self.swarm else { return false };
        s.others.iter().any(|o| super::plan::in_body(*o, p))
    }

    pub(super) fn add_scaffold(&mut self, p: BlockPos) {
        self.scaffold.insert(p);
        if let Some(s) = &self.swarm {
            s.board.add_scaffold(p);
        }
    }

    /// Heartbeat, the others' claims and feet, and scaffold shared both ways.
    pub(super) fn sync(&mut self, cx: &Ctx) {
        let Some(s) = &mut self.swarm else { return };
        if s.beat.is_none_or(|b| cx.tick >= b + BEAT) {
            s.board.heartbeat(&s.who, cx.pos);
            s.beat = Some(cx.tick);
        }
        (s.taken, s.others) = (s.board.taken(&s.who), s.board.others(&s.who));
        let rank = s.board.rank(&s.who);
        if rank != s.home.0 {
            self.cut_home(rank);
        }
        let Some(s) = &self.swarm else { return };
        let board = s.board.clone();
        for p in board.scaffold() {
            match cx.look.solid(p) {
                Some(false) => board.remove_scaffold(p),
                _ => {
                    self.scaffold.insert(p);
                }
            }
        }
        self.scaffold.iter().for_each(|p| board.add_scaffold(*p));
    }

    /// Claims [`Builder::batch`] and the cells acted on within [`GRACE`] in place of the last claim; false:
    /// another bot holds `target`. `acts`: the step acts on `target` now (it starts a grace).
    pub(super) fn claim_batch(&mut self, target: Option<BlockPos>, acts: bool) -> bool {
        let (mut batch, tick) = (std::mem::take(&mut self.batch), self.tick);
        let Some(s) = &mut self.swarm else { return true };
        s.acted.retain(|(_, t)| tick < t + GRACE);
        batch.extend(s.acted.iter().map(|(p, _)| *p).filter(|p| !batch.contains(p)).collect::<Vec<_>>());
        if batch != s.claimed {
            s.claimed = s.board.claim(&s.who, &batch);
            s.taken = s.board.taken(&s.who);
        }
        let ok = target.is_none_or(|p| s.claimed.contains(&p));
        if let Some(p) = target.filter(|_| ok && acts) {
            s.acted.push((p, tick));
        }
        ok
    }

    /// Bots sharing the job (1 without a board).
    pub(super) fn sharers(&self) -> u32 {
        self.swarm.as_ref().map_or(1, |s| s.board.others(&s.who).len() as u32 + 1)
    }

    /// The board lets this bot open `station` now (always without a board).
    pub(super) fn open_station(&self, station: BlockPos) -> bool {
        self.swarm.as_ref().is_none_or(|s| s.board.open_station(&s.who, station))
    }

    pub(super) fn close_station(&self, station: BlockPos) {
        if let Some(s) = &self.swarm {
            s.board.close_station(&s.who, station);
        }
    }

    pub(super) fn leave(&self) {
        if let Some(s) = &self.swarm {
            s.board.release(&s.who);
        }
    }
}
