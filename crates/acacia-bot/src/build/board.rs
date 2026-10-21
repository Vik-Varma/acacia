//! Shared work for several builders on one job: the cells each bot is about to work on, claimed under expiring
//! leases, stations opened one bot at a time, and the scaffold every bot placed. Progress itself is never on the
//! board: a claim that lapses leaves cells that whoever comes next surveys again. Design: acacia-pathfind/docs/builder.md "Swarm".

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use acacia_physics::{BlockPos, Vec3};

/// The board builders on one job share (in one process: [`LocalBoard`]; a networked one can implement it too).
pub trait TaskBoard: Send + Sync {
    /// Of `wanted`, the cells no other bot holds, now held by `who` in place of what it held before.
    fn claim(&self, who: &str, wanted: &[BlockPos]) -> Vec<BlockPos>;
    /// Renews `who`'s leases and posts where it stands.
    fn heartbeat(&self, who: &str, feet: Vec3);
    fn release(&self, who: &str);
    /// Cells other bots hold.
    fn taken(&self, who: &str) -> HashSet<BlockPos>;
    /// Feet of the other bots with live leases.
    fn others(&self, who: &str) -> Vec<Vec3>;
    /// `who`'s place among the bots with live leases, by name, and how many there are.
    fn rank(&self, who: &str) -> (usize, usize);
    /// Takes `station` for `who` until closed or its lease runs out; false: another bot has it open.
    fn open_station(&self, who: &str, station: BlockPos) -> bool;
    fn close_station(&self, who: &str, station: BlockPos);
    fn add_scaffold(&self, pos: BlockPos);
    fn remove_scaffold(&self, pos: BlockPos);
    fn scaffold(&self) -> Vec<BlockPos>;
}

enum Clock {
    /// Milliseconds since creation, in vanilla-tick time (`ACACIA_TICK_SCALE`).
    Wall(Instant),
    /// Set by the caller (simulations count ticks).
    Manual(AtomicU64),
}

struct Bot {
    cells: HashSet<BlockPos>,
    feet: Vec3,
    until: u64,
}

#[derive(Default)]
struct State {
    bots: HashMap<String, Bot>,
    stations: HashMap<BlockPos, (String, u64)>,
    scaffold: HashSet<BlockPos>,
}

impl State {
    fn bot(&mut self, who: &str, until: u64) -> &mut Bot {
        let bot = self.bots.entry(who.to_string()).or_insert(Bot { cells: HashSet::new(), feet: [f32::NAN; 3], until });
        bot.until = until;
        bot
    }

    fn taken(&self, who: &str) -> HashSet<BlockPos> {
        self.bots.iter().filter(|(k, _)| *k != who).flat_map(|(_, b)| b.cells.iter().copied()).collect()
    }
}

/// A [`TaskBoard`] for builders in one process.
pub struct LocalBoard {
    clock: Clock,
    lease: u64,
    state: Mutex<State>,
}

impl LocalBoard {
    /// Leases last `lease_ms` milliseconds after the last heartbeat.
    pub fn new(lease_ms: u64) -> Self {
        Self { clock: Clock::Wall(Instant::now()), lease: lease_ms, state: Mutex::default() }
    }

    /// Leases last `lease` units of a clock moved by [`LocalBoard::set_now`].
    pub fn manual(lease: u64) -> Self {
        Self { clock: Clock::Manual(AtomicU64::new(0)), lease, state: Mutex::default() }
    }

    pub fn set_now(&self, now: u64) {
        if let Clock::Manual(t) = &self.clock {
            t.store(now, Ordering::Relaxed);
        }
    }

    fn now(&self) -> u64 {
        match &self.clock {
            Clock::Wall(t) => crate::cadence::unscaled(t.elapsed()).as_millis() as u64,
            Clock::Manual(t) => t.load(Ordering::Relaxed),
        }
    }

    /// The state with expired leases dropped, and when a lease taken now would run out.
    fn live(&self) -> (std::sync::MutexGuard<'_, State>, u64) {
        let now = self.now();
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.bots.retain(|_, b| b.until > now);
        s.stations.retain(|_, (_, until)| *until > now);
        (s, now + self.lease)
    }
}

impl TaskBoard for LocalBoard {
    fn claim(&self, who: &str, wanted: &[BlockPos]) -> Vec<BlockPos> {
        let (mut s, until) = self.live();
        let taken = s.taken(who);
        let got: Vec<BlockPos> = wanted.iter().copied().filter(|c| !taken.contains(c)).collect();
        s.bot(who, until).cells = got.iter().copied().collect();
        got
    }

    fn heartbeat(&self, who: &str, feet: Vec3) {
        let (mut s, until) = self.live();
        s.bot(who, until).feet = feet;
        s.stations.values_mut().filter(|(k, _)| k == who).for_each(|(_, u)| *u = until);
    }

    fn release(&self, who: &str) {
        if let Some(b) = self.live().0.bots.get_mut(who) {
            b.cells.clear();
        }
    }

    fn taken(&self, who: &str) -> HashSet<BlockPos> {
        self.live().0.taken(who)
    }

    fn others(&self, who: &str) -> Vec<Vec3> {
        self.live().0.bots.iter().filter(|(k, _)| *k != who).map(|(_, b)| b.feet).collect()
    }

    fn rank(&self, who: &str) -> (usize, usize) {
        let s = self.live().0;
        (s.bots.keys().filter(|k| k.as_str() < who).count(), s.bots.len().max(1))
    }

    fn open_station(&self, who: &str, station: BlockPos) -> bool {
        let (mut s, until) = self.live();
        match s.stations.get(&station) {
            Some((k, _)) if k != who => false,
            _ => {
                s.stations.insert(station, (who.to_string(), until));
                true
            }
        }
    }

    fn close_station(&self, who: &str, station: BlockPos) {
        let (mut s, _) = self.live();
        if s.stations.get(&station).is_some_and(|(k, _)| k == who) {
            s.stations.remove(&station);
        }
    }

    fn add_scaffold(&self, pos: BlockPos) {
        self.live().0.scaffold.insert(pos);
    }

    fn remove_scaffold(&self, pos: BlockPos) {
        self.live().0.scaffold.remove(&pos);
    }

    fn scaffold(&self) -> Vec<BlockPos> {
        self.live().0.scaffold.iter().copied().collect()
    }
}
