//! Shared work for several builders on one job: footprint regions claimed under expiring leases, stations opened
//! one bot at a time, and the scaffold every bot placed. Progress itself is never on the board: a region that
//! comes back to the pool is surveyed again. Design: acacia-pathfind/docs/builder.md "Swarm".

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use acacia_physics::{BlockPos, Vec3};

/// A footprint tile, by tile index along x and z.
pub type RegionId = (i32, i32);

/// The board builders on one job share (in one process: [`LocalBoard`]; a networked one can implement it too).
pub trait TaskBoard: Send + Sync {
    /// The first of `wanted` not leased to another bot, now leased to `who` (dropping any other region it held).
    fn claim(&self, who: &str, wanted: &[RegionId]) -> Option<RegionId>;
    /// Renews `who`'s leases and posts where it stands; returns the region it still holds.
    fn heartbeat(&self, who: &str, feet: Vec3) -> Option<RegionId>;
    fn release(&self, who: &str);
    /// Regions leased to other bots.
    fn taken(&self, who: &str) -> HashSet<RegionId>;
    /// Feet of the other bots with live leases.
    fn others(&self, who: &str) -> Vec<Vec3>;
    /// Takes `station` for `who` until closed or its lease runs out; false: another bot has it open.
    fn open_station(&self, who: &str, station: BlockPos) -> bool;
    fn close_station(&self, who: &str, station: BlockPos);
    fn add_scaffold(&self, pos: BlockPos);
    fn remove_scaffold(&self, pos: BlockPos);
    fn scaffold(&self) -> Vec<BlockPos>;
}

enum Clock {
    /// Milliseconds since creation.
    Wall(Instant),
    /// Set by the caller (simulations count ticks).
    Manual(AtomicU64),
}

struct Bot {
    region: Option<RegionId>,
    feet: Vec3,
    until: u64,
}

#[derive(Default)]
struct State {
    bots: HashMap<String, Bot>,
    stations: HashMap<BlockPos, (String, u64)>,
    scaffold: HashSet<BlockPos>,
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
            Clock::Wall(t) => t.elapsed().as_millis() as u64,
            Clock::Manual(t) => t.load(Ordering::Relaxed),
        }
    }

    /// The state with expired leases dropped.
    fn live(&self) -> (std::sync::MutexGuard<'_, State>, u64) {
        let now = self.now();
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.bots.retain(|_, b| b.until > now);
        s.stations.retain(|_, (_, until)| *until > now);
        (s, now)
    }
}

impl TaskBoard for LocalBoard {
    fn claim(&self, who: &str, wanted: &[RegionId]) -> Option<RegionId> {
        let (mut s, now) = self.live();
        let taken: HashSet<RegionId> = s.bots.iter().filter(|(k, _)| *k != who).filter_map(|(_, b)| b.region).collect();
        let pick = wanted.iter().copied().find(|r| !taken.contains(r));
        let until = now + self.lease;
        let bot = s.bots.entry(who.to_string()).or_insert(Bot { region: None, feet: [0.0; 3], until });
        (bot.region, bot.until) = (pick, until);
        pick
    }

    fn heartbeat(&self, who: &str, feet: Vec3) -> Option<RegionId> {
        let (mut s, now) = self.live();
        let until = now + self.lease;
        let bot = s.bots.entry(who.to_string()).or_insert(Bot { region: None, feet, until });
        (bot.feet, bot.until) = (feet, until);
        let region = bot.region;
        s.stations.values_mut().filter(|(k, _)| k == who).for_each(|(_, u)| *u = until);
        region
    }

    fn release(&self, who: &str) {
        if let Some(b) = self.live().0.bots.get_mut(who) {
            b.region = None;
        }
    }

    fn taken(&self, who: &str) -> HashSet<RegionId> {
        self.live().0.bots.iter().filter(|(k, _)| *k != who).filter_map(|(_, b)| b.region).collect()
    }

    fn others(&self, who: &str) -> Vec<Vec3> {
        self.live().0.bots.iter().filter(|(k, _)| *k != who).map(|(_, b)| b.feet).collect()
    }

    fn open_station(&self, who: &str, station: BlockPos) -> bool {
        let (mut s, now) = self.live();
        match s.stations.get(&station) {
            Some((k, _)) if k != who => false,
            _ => {
                s.stations.insert(station, (who.to_string(), now + self.lease));
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
