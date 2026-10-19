//! A builder sharing its job through a [`TaskBoard`]: it acts only inside the region it holds, keeps out of the
//! regions others hold, and gives its region back when nothing in it can be done for now.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use acacia_physics::{BlockPos, Vec3};

use super::board::{RegionId, TaskBoard};
use super::builder::Builder;
use super::survey::Need;
use crate::pathfind::process::Ctx;

/// Ticks between heartbeats (the lease must outlast several).
const BEAT: u64 = 20;
/// Ticks a region with nothing doable is left before it is claimed again.
const SKIP: u64 = 60;
/// Stands closer than this (horizontally) to another bot are not used.
const ROOM: f32 = 1.5;

pub(super) struct Swarm {
    board: Arc<dyn TaskBoard>,
    who: String,
    held: Option<RegionId>,
    skip: HashMap<RegionId, u64>,
    beat: Option<u64>,
    taken: HashSet<RegionId>,
    others: Vec<Vec3>,
}

impl Builder {
    /// Shares the job with other builders on `board`, under the name `who` (unique per bot).
    pub fn with_board(mut self, board: Arc<dyn TaskBoard>, who: impl Into<String>) -> Self {
        let who = who.into();
        // On the board from now (feet unknown: NaN crowds nobody), so others count this bot from their first restock.
        board.heartbeat(&who, [f32::NAN; 3]);
        let swarm = Swarm { board, who, held: None, skip: HashMap::new(), beat: None, taken: HashSet::new(), others: Vec::new() };
        self.swarm = Some(swarm);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_tile(mut self, tile: i32) -> Self {
        self.job.tile = tile;
        self
    }

    pub(super) fn region(&self, p: BlockPos) -> RegionId {
        let (lo, hi, t) = (self.lo, self.hi, self.job.tile.max(1));
        ((p[0].clamp(lo[0], hi[0]) - lo[0]).div_euclid(t), (p[2].clamp(lo[1], hi[1]) - lo[1]).div_euclid(t))
    }

    /// The cell is this bot's to work on: no board, or in the region it holds.
    pub(super) fn mine(&self, p: BlockPos) -> bool {
        self.swarm.as_ref().is_none_or(|s| s.held == Some(self.region(p)))
    }

    /// The cell is in a region another bot holds.
    pub(super) fn theirs(&self, p: BlockPos) -> bool {
        self.swarm.as_ref().is_some_and(|s| s.taken.contains(&self.region(p)))
    }

    /// Feet in `cell` would crowd another bot.
    pub(super) fn crowded(&self, cell: BlockPos) -> bool {
        let Some(s) = &self.swarm else { return false };
        let c = [cell[0] as f32 + 0.5, cell[1] as f32, cell[2] as f32 + 0.5];
        s.others.iter().any(|o| (o[0] - c[0]).hypot(o[2] - c[2]) < ROOM && (o[1] - c[1]).abs() < 2.0)
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

    /// Heartbeat, scaffold shared both ways, and a region claimed when none is held.
    pub(super) fn sync(&mut self, cx: &Ctx, needs: &[Need]) {
        let Some(s) = &mut self.swarm else { return };
        if s.beat.is_none_or(|b| cx.tick >= b + BEAT) {
            s.held = s.board.heartbeat(&s.who, cx.pos);
            (s.taken, s.others, s.beat) = (s.board.taken(&s.who), s.board.others(&s.who), Some(cx.tick));
        }
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
        if self.swarm.as_ref().is_some_and(|s| s.held.is_none()) {
            self.claim(cx, needs);
        }
    }

    fn claim(&mut self, cx: &Ctx, needs: &[Need]) {
        let tick = cx.tick;
        let mut open: HashSet<RegionId> = self.cells.iter().zip(needs).filter(|(c, n)| matches!(n, Need::Place | Need::Break | Need::Drain | Need::Unknown) && !self.given_up(c.pos)).map(|(c, _)| self.region(c.pos)).collect();
        open.extend(self.scaffold.iter().filter(|p| cx.look.solid(**p) == Some(true) && self.target(**p).is_none_or(|t| t.is_air())).map(|p| self.region(*p)));
        let (lo, t) = (self.lo, self.job.tile.max(1) as f32);
        let centre = |r: RegionId| [lo[0] as f32 + (r.0 as f32 + 0.5) * t, lo[1] as f32 + (r.1 as f32 + 0.5) * t];
        let Some(s) = &mut self.swarm else { return };
        s.skip.retain(|_, until| *until > tick);
        let mut wanted: Vec<RegionId> = open.into_iter().filter(|r| !s.skip.contains_key(r)).collect();
        let d = |r: RegionId| (centre(r)[0] - cx.pos[0]).hypot(centre(r)[1] - cx.pos[2]);
        wanted.sort_by(|a, b| d(*a).total_cmp(&d(*b)).then(a.cmp(b)));
        s.held = s.board.claim(&s.who, &wanted);
        s.taken = s.board.taken(&s.who);
        if s.held.is_some() {
            tracing::debug!(who = %s.who, region = ?s.held, "build region claimed");
        }
    }

    /// Nothing in the held region can be done now: give it back and leave it a while.
    pub(super) fn exhausted(&mut self) {
        let tick = self.tick;
        let Some(s) = &mut self.swarm else { return };
        if let Some(r) = s.held.take() {
            s.skip.insert(r, tick + SKIP);
            s.board.release(&s.who);
            self.commit = None;
        }
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
