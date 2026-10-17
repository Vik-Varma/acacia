//! The schematic builder as a process: each tick it re-surveys the footprint and picks one thing to do. Design:
//! acacia-pathfind/docs/builder.md.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use acacia_physics::BlockPos;

use super::job::BuildJob;
use super::plan::{self, Next};
use super::status::{BuildStatus, Phase, StatusHandle};
use super::structure::{Block, Structure, StructureError};
use super::survey::{need, Need};
use crate::pathfind::process::{Act, Ctx, Event, Look, Process, Want};
use crate::pathfind::GotoOpts;
use crate::ActionError;

/// Failed acts on one cell before it is given up.
const MAX_FAILS: u32 = 4;
/// Placements that came out wrong (and were broken again) before a cell is given up.
const MAX_PLACES: u32 = 3;
const RETRY_TICKS: u64 = 20;
const REST_TICKS: u64 = 40;
/// Rests (40 ticks each) before a cell nothing reaches is given up: a minute and a half.
const MAX_RESTS: u32 = 45;

pub(super) struct Cell {
    pub pos: BlockPos,
    pub target: Block,
}

#[derive(Default)]
pub(super) struct Tries {
    pub fails: u32,
    pub places: u32,
    /// Times nowhere was found to do it from.
    pub rests: u32,
    pub next_at: u64,
}

impl Tries {
    fn given_up(&self) -> bool {
        self.fails >= MAX_FAILS || self.places > MAX_PLACES || self.rests > MAX_RESTS
    }
}

pub(super) struct Station {
    pub pos: BlockPos,
    /// Items it was found without.
    pub lacks: HashSet<String>,
}

/// A restock in flight: the counts before it, to learn what the station lacks.
pub(super) struct Restock {
    pub station: usize,
    pub before: Vec<(String, u32)>,
}

pub struct Builder {
    pub(super) job: BuildJob,
    pub(super) cells: Vec<Cell>,
    pub(super) index: HashMap<BlockPos, usize>,
    pub(super) tries: HashMap<BlockPos, Tries>,
    /// Temporary blocks: pillars and supports, removed at the end.
    pub(super) scaffold: HashSet<BlockPos>,
    /// Blocks holding liquids back from outside the footprint: left in place.
    pub(super) seals: HashSet<BlockPos>,
    pub(super) stations: Vec<Station>,
    pub(super) restock: Option<Restock>,
    /// The goal being driven to, kept while its task is open (a new pilot each tick would never get there).
    pub(super) commit: Option<(BlockPos, crate::pathfind::Goal)>,
    /// The goal the pilot reached: asking for it again would only hold still (the arbiter keeps an arrived route).
    arrived: Option<crate::pathfind::Goal>,
    pub(super) tick: u64,
    pub(super) waiting_since: Option<u64>,
    status: StatusHandle,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("reading {0}: {1}")]
    Io(String, std::io::Error),
    #[error(transparent)]
    Structure(#[from] StructureError),
}

impl Builder {
    /// Loads the job's structure file.
    pub fn new(job: BuildJob) -> Result<Self, BuildError> {
        let bytes = std::fs::read(&job.structure).map_err(|e| BuildError::Io(job.structure.display().to_string(), e))?;
        Ok(Self::with_structure(job, &Structure::load(&bytes)?))
    }

    /// Builds `structure` where `job` says (its file is not read).
    pub fn with_structure(job: BuildJob, structure: &Structure) -> Self {
        let cells: Vec<Cell> = structure
            .cells()
            .filter(|(_, b)| b.name != "minecraft:structure_void")
            .map(|(local, b)| Cell { pos: job.at.apply(structure.size, local), target: job.at.block(b) })
            .collect();
        let index = cells.iter().enumerate().map(|(i, c)| (c.pos, i)).collect();
        let stations = job.stations.iter().map(|&pos| Station { pos, lacks: HashSet::new() }).collect();
        let status = Arc::new(Mutex::new(BuildStatus { cells: cells.len(), ..BuildStatus::default() }));
        Self {
            job,
            cells,
            index,
            tries: HashMap::new(),
            scaffold: HashSet::new(),
            seals: HashSet::new(),
            stations,
            restock: None,
            commit: None,
            arrived: None,
            tick: 0,
            waiting_since: None,
            status,
        }
    }

    /// Progress, readable while the builder runs inside [`crate::pathfind::Processes`].
    pub fn status(&self) -> StatusHandle {
        self.status.clone()
    }

    /// Why each open placement is not being done from `feet` (tests).
    #[cfg(test)]
    pub(crate) fn explain(&self, look: &dyn Look, feet: acacia_physics::Vec3) -> Vec<String> {
        plan::explain(self, look, feet)
    }

    /// Diagnostics: each open placement in `bot`'s world, with the click it has from where the bot stands and the
    /// stands it could use (a builder for the same job, outside the processes).
    pub fn explain_for(&self, bot: &crate::Bot) -> Vec<String> {
        let Some(feet) = bot.movement().and_then(|m| m.position()) else { return Vec::new() };
        plan::explain(self, &crate::pathfind::process::run::BotLook(bot), feet)
    }

    pub(super) fn target(&self, pos: BlockPos) -> Option<&Block> {
        self.index.get(&pos).map(|&i| &self.cells[i].target)
    }

    /// A cell to leave alone for now: given up on, or waiting out a retry.
    pub(super) fn resting(&self, pos: BlockPos) -> bool {
        self.tries.get(&pos).is_some_and(|t| t.given_up() || t.next_at > self.tick)
    }

    /// Nowhere to do the cell from yet: let others go first for a while (not a failure).
    pub(super) fn rest(&mut self, pos: BlockPos) {
        let t = self.tries.entry(pos).or_default();
        (t.rests, t.next_at) = (t.rests + 1, self.tick + REST_TICKS);
    }

    pub(super) fn given_up(&self, pos: BlockPos) -> bool {
        self.tries.get(&pos).is_some_and(Tries::given_up)
    }

    fn fail(&mut self, pos: BlockPos, e: &ActionError) {
        tracing::debug!(?pos, %e, "build act failed");
        let t = self.tries.entry(pos).or_default();
        (t.fails, t.next_at) = (t.fails + 1, self.tick + RETRY_TICKS);
        if self.commit.as_ref().is_some_and(|(p, _)| *p == pos) {
            self.commit = None;
        }
    }

    pub(super) fn survey(&self, look: &dyn Look) -> Vec<Need> {
        self.cells.iter().map(|c| need(&c.target, look.block(c.pos), self.job.fill_only)).collect()
    }

    /// Learns what a station lacks from what the last restock brought.
    fn check_restock(&mut self, look: &dyn Look) {
        let Some(r) = self.restock.take() else { return };
        for (item, before) in r.before {
            if look.count(&item) <= before {
                self.stations[r.station].lacks.insert(item);
            }
        }
    }

    fn publish(&self, needs: &[Need], look: &dyn Look, phase: Phase, missing: Vec<String>) {
        let Ok(mut s) = self.status.lock() else { return };
        let count = |n: Need| needs.iter().filter(|x| **x == n).count();
        (s.done, s.to_place, s.to_break, s.unknown, s.kept) = (count(Need::Done), count(Need::Place), count(Need::Break) + count(Need::Drain), count(Need::Unknown), count(Need::Kept));
        s.unsupported = self.cells.iter().zip(needs).filter(|(_, n)| **n == Need::Unsupported).map(|(c, _)| (c.pos, c.target.name.clone())).collect();
        s.failed = self.tries.iter().filter(|(_, t)| t.given_up()).map(|(p, _)| *p).collect();
        s.scaffold = self.scaffold.iter().filter(|p| look.solid(**p) == Some(true)).count();
        s.seals = self.seals.iter().copied().collect();
        (s.phase, s.missing) = (phase, missing);
    }

    fn bump(&self, f: impl FnOnce(&mut BuildStatus)) {
        if let Ok(mut s) = self.status.lock() {
            f(&mut s);
        }
    }
}

impl Process for Builder {
    fn name(&self) -> &str {
        "builder"
    }

    fn want(&mut self, cx: &Ctx) -> Want {
        self.tick = cx.tick;
        self.check_restock(cx.look);
        let needs = self.survey(cx.look);
        self.scaffold.retain(|p| cx.look.solid(*p) != Some(false));
        // In the air: keep driving (a jump or pillar in progress), or wait to land before acting.
        let mut next = match &self.commit {
            Some((task, goal)) if !cx.on_ground => Next::Go(Phase::Building, *task, goal.clone()),
            _ => Next::Hold(Phase::Building, Vec::new()),
        };
        // A task wanting the goal just reached could not act from there after all: let it rest and ask again.
        for _ in 0..8 {
            if !cx.on_ground {
                break;
            }
            next = plan::next(self, cx, &needs);
            match &next {
                Next::Go(_, task, goal) if self.arrived.as_ref() == Some(goal) => {
                    tracing::debug!(?task, ?goal, "build stand reached but no click from there");
                    self.rest(*task);
                    next = Next::Hold(Phase::Building, Vec::new());
                }
                _ => break,
            }
        }
        let (phase, missing, want) = match next {
            Next::Act(phase, act) => {
                self.arrived = None;
                (phase, Vec::new(), Want::Act(act))
            }
            Next::Go(phase, task, goal) => {
                self.commit = Some((task, goal.clone()));
                (phase, Vec::new(), Want::Go(goal))
            }
            Next::Hold(phase, missing) => (phase, missing, Want::Hold),
            Next::Done(result) => (Phase::Done, Vec::new(), Want::Done(result)),
        };
        if phase == Phase::Waiting {
            let since = *self.waiting_since.get_or_insert(self.tick);
            // Stations may have been refilled since: ask them again now and then.
            if self.tick - since > 600 {
                self.stations.iter_mut().for_each(|s| s.lacks.clear());
                self.waiting_since = None;
            }
        } else {
            self.waiting_since = None;
        }
        self.publish(&needs, cx.look, phase, missing);
        want
    }

    fn opts(&self) -> GotoOpts {
        let mut opts = GotoOpts { scaffold: self.job.scaffold.clone(), ..GotoOpts::default() };
        opts.search.allow_bridge = true;
        opts
    }

    fn arrived(&mut self) -> bool {
        self.arrived = self.commit.take().map(|(_, goal)| goal);
        false
    }

    fn failed(&mut self, e: &ActionError) -> bool {
        if let Some((task, _)) = self.commit.take() {
            self.fail(task, e);
        }
        false
    }

    fn acted(&mut self, act: &Act, result: &Result<(), ActionError>) {
        match (act, result) {
            (Act::Place { against, face, .. }, Ok(())) => {
                self.tries.entry(face.adjacent(*against)).or_default().places += 1;
                self.bump(|s| s.placed += 1);
            }
            (Act::Place { against, face, .. }, Err(e)) => self.fail(face.adjacent(*against), e),
            (Act::Break { .. }, Ok(())) => self.bump(|s| s.broken += 1),
            (Act::Break { pos }, Err(e)) => self.fail(*pos, e),
            (Act::Fill { pos, .. }, Ok(())) => {
                if self.index.contains_key(pos) || !self.seals.contains(pos) {
                    self.scaffold.insert(*pos);
                }
            }
            (Act::Fill { pos, .. }, Err(e)) => {
                self.seals.remove(pos);
                self.fail(*pos, e);
            }
            (Act::Restock { .. }, r) => {
                if let Err(e) = r {
                    tracing::debug!(%e, "restock failed");
                }
                self.bump(|s| s.restocks += 1);
            }
        }
    }

    fn observe(&mut self, event: &Event) {
        if let Event::Placed { pos } = event {
            self.scaffold.insert(*pos);
        }
    }

    fn lost_control(&mut self) {
        self.commit = None;
    }
}
