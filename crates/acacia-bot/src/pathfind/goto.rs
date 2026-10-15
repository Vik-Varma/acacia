//! Async execution: plan, follow, re-plan on trouble (azalea `execute/`).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use acacia_physics::{PlayerState, Vec3};
use acacia_world::{BlockRegistry, ChunkView};
use tokio::task::JoinHandle;

use super::astar::{search_cached, Path, SearchOpts};
use super::cache::RolloutCache;
use super::follow::{FollowStatus, Follower, Sense};
use super::execute;
use super::goal::Goal;
use super::kit::{DEFAULT_SCAFFOLD, Kit};
use super::snapshot::WorldBlocks;
use super::terrain::{Blocks, Terrain};
use crate::cadence::{seed, splitmix};
use crate::{ActionError, Bot};

/// Nodes ahead re-checked every tick for world changes.
const LOOKAHEAD: usize = 6;
/// A search that finishes within this runs inline, before the next tick (azalea's instant path).
const QUICK_NODES: usize = 5_000;
const QUICK_BUDGET: Duration = Duration::from_millis(10);
/// Macro rollout ticks the inline search may simulate (~5 µs each in release).
const QUICK_ROLLOUT_TICKS: u64 = 1_000;
/// Divergences in a row that re-plan for free; each one past it counts against `max_replans`.
const MAX_DIVERGED: u32 = 5;
const HOP_MIN_FOOD: f32 = 6.0;

#[derive(Debug, Clone)]
pub struct GotoOpts {
    /// Search settings; its `kit` is refilled from the inventory before every search.
    pub search: SearchOpts,
    /// Re-plans after getting stuck, knocked off the path, finding no path or failing a dig, place
    /// or door before giving up. Continuing from the end of a partial path and re-planning after
    /// a teleport or a block change are free.
    pub max_replans: u32,
    /// How close to the final node's centre (horizontally, blocks) counts as arrived.
    pub tolerance: f32,
    /// Item identifiers of the throwaway blocks bridging and pillaring may use, in preference order.
    pub scaffold: Vec<String>,
    /// Run straight at nodes further on over flat ground instead of the path's 8-direction staircase.
    pub shortcuts: bool,
    /// Seeds the followers' aim jitter (`Human`); random when `None`.
    pub seed: Option<u64>,
}

impl Default for GotoOpts {
    fn default() -> Self {
        let scaffold = DEFAULT_SCAFFOLD.iter().map(|s| s.to_string()).collect();
        Self { search: SearchOpts::default(), max_replans: 10, tolerance: 0.25, scaffold, shortcuts: true, seed: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavStatus {
    Searching,
    Moving,
    /// Digging, placing or opening a door for the next move.
    Working,
    Arrived,
}

/// Step-wise [`Bot::goto`]: call [`Navigator::tick`] until it returns `Arrived`, reading progress
/// in between. Dropping it mid-way leaves the controls set; call [`Bot::stop_pathing`].
pub struct Navigator {
    goal: Goal,
    opts: GotoOpts,
    follower: Option<Follower>,
    partial: bool,
    pending: Option<JoinHandle<Path>>,
    replans: u32,
    teleports: Option<u32>,
    /// Macro moves diverged in a row; free re-plans up to `MAX_DIVERGED`.
    diverged: u32,
    diverged_in_air: bool,
    divergences: u32,
    /// Macro-move results kept across re-plans; the blocking search takes it along.
    cache: Arc<Mutex<RolloutCache>>,
    /// Seeds each follower's aim jitter.
    rng: u64,
}

impl Navigator {
    pub fn new(goal: Goal, opts: GotoOpts) -> Self {
        let (cache, rng) = (Arc::default(), opts.seed.unwrap_or_else(seed));
        Self { goal, opts, follower: None, partial: false, pending: None, replans: 0, teleports: None, diverged: 0, diverged_in_air: false, divergences: 0, cache, rng }
    }

    /// Macro moves that diverged live (each re-planned without stopping).
    pub fn divergences(&self) -> u32 {
        self.divergences
    }

    pub fn goal(&self) -> &Goal {
        &self.goal
    }

    /// Re-plans counted against [`GotoOpts::max_replans`].
    pub fn replans(&self) -> u32 {
        self.replans
    }

    /// Path nodes still ahead (0 while searching).
    pub fn remaining(&self) -> usize {
        self.follower.as_ref().map_or(0, Follower::remaining)
    }

    /// The current path leads towards the goal but not to it.
    pub fn is_partial(&self) -> bool {
        self.partial
    }

    pub fn follower(&self) -> Option<&Follower> {
        self.follower.as_ref()
    }

    /// Sets the controls for one tick and lets it pass (except on arrival). Controls are stopped
    /// on arrival and on error.
    pub async fn tick(&mut self, bot: &mut Bot) -> Result<NavStatus, ActionError> {
        let status = match self.poll(bot).await {
            Ok(s) => s,
            Err(e) => {
                bot.stop_pathing();
                return Err(e);
            }
        };
        if status == NavStatus::Arrived {
            bot.stop_pathing();
        } else {
            bot.next_tick(|_, _| false).await?;
        }
        Ok(status)
    }

    async fn poll(&mut self, bot: &mut Bot) -> Result<NavStatus, ActionError> {
        if bot.movement.is_none() {
            return Err(ActionError::NotPossible("pathfinding needs a physics bot (BotConfig.physics)".into()));
        }
        if let Some(handle) = &mut self.pending {
            if !handle.is_finished() {
                return Ok(NavStatus::Searching);
            }
            let path = handle.await.map_err(|e| ActionError::NotPossible(format!("path search failed: {e}")))?;
            self.pending = None;
            let pos = bot.movement.as_ref().and_then(|m| m.position()).unwrap_or_default();
            self.accept(path, pos)?;
        }
        let status = self.drive(bot)?;
        if let Some(node) = self.follower.as_ref().and_then(Follower::pending_work).copied()
            && status == NavStatus::Working
        {
            match execute::perform(bot, &node, &self.opts.scaffold).await {
                Ok(()) => self.follower.as_mut().into_iter().for_each(Follower::work_done),
                Err(ActionError::Disconnected) => return Err(ActionError::Disconnected),
                Err(e) => {
                    tracing::debug!(%e, ?node, "path work failed; re-planning");
                    self.follower = None;
                    self.count_replan(e.to_string())?;
                }
            }
        }
        Ok(status)
    }

    fn drive(&mut self, bot: &mut Bot) -> Result<NavStatus, ActionError> {
        let s = &self.opts.search;
        let kit = (s.allow_dig || s.allow_bridge).then(|| bot.path_kit(&self.opts.scaffold));
        let opts = self.search_opts(kit, bot.state.player.hunger);
        let (Some(world), Some(movement)) = (bot.world.as_ref(), bot.movement.as_mut()) else {
            return Ok(NavStatus::Searching);
        };
        let (Some(view), Some(registry), Some(state)) = (world.view(), world.registry(), movement.state()) else {
            return Ok(NavStatus::Searching);
        };
        let (pos, live_state) = (state.pos, state.clone());
        let terrain = Terrain::new(view, registry.as_ref());
        if self.teleports.replace(movement.teleports).is_some_and(|t| t != movement.teleports) {
            self.follower = None;
        }
        if self.follower.as_ref().is_some_and(|f| !still_valid(&terrain, f)) {
            tracing::debug!("path blocked by a world change; re-planning");
            self.follower = None;
        }
        let Some(follower) = &mut self.follower else {
            // After a divergence the controls carry the bot on through the air while it re-plans.
            if !self.diverged_in_air || movement.on_ground() {
                movement.controls.stop();
            }
            self.plan(&terrain, view, registry, live_state, opts, true)?;
            return Ok(NavStatus::Searching);
        };
        let feet = [pos[0].floor() as i32, pos[1].floor() as i32, pos[2].floor() as i32];
        let sense = Sense { pos, on_ground: movement.on_ground(), in_water: terrain.cell(feet).is_water() };
        let live = movement.fork();
        match follower.tick(&sense, live.as_ref(), &terrain, &mut movement.controls) {
            FollowStatus::Moving => {
                if follower.index() > 0 {
                    self.diverged = 0;
                }
            }
            FollowStatus::Diverged => {
                // Out of rollout budget this tick: no inline search on top.
                let inline = !follower.spend().short;
                tracing::debug!(?pos, inline, "macro move diverged; re-planning in place");
                self.diverged += 1;
                self.divergences += 1;
                if self.diverged > MAX_DIVERGED {
                    self.count_replan(format!("diverged {} times in a row", self.diverged))?;
                    self.diverged = 0;
                }
                self.follower = None;
                self.diverged_in_air = !movement.on_ground();
                self.plan(&terrain, view, registry, live_state, opts, inline)?;
            }
            FollowStatus::Work => return Ok(NavStatus::Working),
            FollowStatus::Arrived if !self.partial => return Ok(NavStatus::Arrived),
            FollowStatus::Arrived => self.follower = None,
            status @ (FollowStatus::Stuck | FollowStatus::OffPath) => {
                tracing::debug!(?status, ?pos, "re-planning");
                self.count_replan(format!("{status:?} {} times", self.replans))?;
                self.follower = None;
            }
        }
        Ok(NavStatus::Moving)
    }

    /// Searches from the live state, so the plan starts with the momentum the bot has (a re-plan after a
    /// divergence keeps running). `inline`: try a quick search before the tick ends, else only in the background.
    fn plan(&mut self, terrain: &Terrain<&ChunkView>, view: &ChunkView, registry: &Arc<BlockRegistry>, live: PlayerState, opts: SearchOpts, inline: bool) -> Result<(), ActionError> {
        if inline {
            let quick_opts = SearchOpts { max_nodes: QUICK_NODES, budget: QUICK_BUDGET, max_rollout_ticks: QUICK_ROLLOUT_TICKS, ..opts.clone() };
            let quick = search_cached(terrain, &live, &self.goal, &quick_opts, &mut lock(&self.cache));
            // A plan whose macro budget ran out may have missed faster macro moves: search again in full.
            if quick.complete && !quick.macro_stats.exhausted {
                return self.accept(quick, live.pos);
            }
        }
        let (world, registry, goal, cache) = (view.world().clone(), registry.clone(), self.goal.clone(), self.cache.clone());
        self.pending = Some(tokio::task::spawn_blocking(move || {
            search_cached(&Terrain::new(WorldBlocks::new(world), &registry), &live, &goal, &opts, &mut lock(&cache))
        }));
        Ok(())
    }

    /// The search settings for now: the inventory's kit, and no flat hops at food 6 or less (sprinting
    /// stops there, and hops burn the most food).
    pub(crate) fn search_opts(&self, kit: Option<Kit>, food: f32) -> SearchOpts {
        let mut opts = self.opts.search.clone();
        if let Some(kit) = kit {
            opts.kit = kit;
        }
        opts.hops &= food > HOP_MIN_FOOD;
        opts
    }

    fn accept(&mut self, path: Path, pos: Vec3) -> Result<(), ActionError> {
        tracing::debug!(nodes = path.nodes.len(), complete = path.complete, cost = path.cost, visited = path.visited, "path found");
        if path.nodes.is_empty() && !path.complete {
            self.count_replan("no path".into())?;
        }
        self.partial = !path.complete;
        self.diverged_in_air = false;
        let follower = Follower::new(path.nodes, pos, self.opts.tolerance).moving(self.opts.search.profile, splitmix(&mut self.rng));
        self.follower = Some(follower.shortcuts(self.opts.shortcuts));
        Ok(())
    }

    fn count_replan(&mut self, why: String) -> Result<(), ActionError> {
        self.replans += 1;
        if self.replans > self.opts.max_replans {
            return Err(ActionError::NotPossible(format!("cannot reach {:?}: {why}", self.goal)));
        }
        Ok(())
    }
}

fn lock(cache: &Mutex<RolloutCache>) -> MutexGuard<'_, RolloutCache> {
    cache.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The next nodes still offer the same footing (up to the first that waits for work: its footing
/// comes from that work).
fn still_valid<B: Blocks>(terrain: &Terrain<B>, f: &Follower) -> bool {
    f.standing_ahead().iter().take(LOOKAHEAD).all(|n| terrain.spot(n.pos).is_some_and(|s| (s.feet - n.feet).abs() < 0.01))
}
