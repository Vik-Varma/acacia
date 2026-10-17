//! [`Arbiter`] on a server: the bot's world and entities to look at, its actions to drive and act with.

use acacia_physics::{BlockPos, Vec3};

use super::arbiter::{Arbiter, Decision};
use super::act;
use super::{BlockView, Look, ProcId, Process, Target};
use crate::pathfind::goto::step;
use crate::pathfind::{NavStatus, Pilot};
use crate::{ActionError, Bot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Driving to the goal of the process in control.
    Moving(NavStatus),
    /// Standing still for the process in control.
    Holding,
    /// The process in control placed or broke something.
    Acted,
    /// Every process yields.
    Idle,
    /// No processes left.
    Empty,
}

/// Processes run by one bot: add them with a priority (higher wins), then call [`Processes::tick`] once
/// per tick or [`Bot::run`] until every process has finished.
#[derive(Default)]
pub struct Processes {
    arbiter: Arbiter,
}

impl Processes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, proc: impl Process + 'static, priority: i32) -> ProcId {
        self.arbiter.add(proc, priority)
    }

    pub fn pause(&mut self, id: ProcId) -> bool {
        self.arbiter.pause(id)
    }

    pub fn resume(&mut self, id: ProcId) -> bool {
        self.arbiter.resume(id)
    }

    pub fn remove(&mut self, id: ProcId) -> Option<Box<dyn Process>> {
        self.arbiter.remove(id)
    }

    pub fn arbiter(&self) -> &Arbiter {
        &self.arbiter
    }

    pub fn outcome(&self, id: ProcId) -> Option<&Result<(), ActionError>> {
        self.arbiter.outcome(id)
    }

    pub fn take_outcome(&mut self, id: ProcId) -> Option<Result<(), ActionError>> {
        self.arbiter.take_outcome(id)
    }

    pub fn pilot(&self) -> Option<&Pilot> {
        self.arbiter.pilot()
    }

    /// Gives control to one process for a tick and lets the tick pass (not once every process has finished).
    /// Errors only when the bot cannot move or is disconnected; processes' own failures are their outcomes.
    pub async fn tick(&mut self, bot: &mut Bot) -> Result<Status, ActionError> {
        let Some(movement) = bot.movement.as_ref() else {
            return Err(ActionError::NotPossible("processes need a physics bot (BotConfig.physics)".into()));
        };
        let Some(live) = movement.state() else {
            bot.next_tick(|_, _| false).await?;
            return Ok(Status::Idle);
        };
        let (pos, on_ground) = (live.pos, movement.on_ground());
        let decision = self.arbiter.decide(pos, on_ground, &BotLook(bot));
        if matches!(decision, Decision::Hold | Decision::Idle | Decision::Empty) {
            bot.stop_pathing();
        }
        let status = match decision {
            Decision::Go => {
                let Some(pilot) = self.arbiter.pilot_mut() else { return Ok(Status::Idle) };
                let (result, worked) = step(pilot, bot).await;
                if result == Err(ActionError::Disconnected) {
                    return Err(ActionError::Disconnected);
                }
                let status = result.as_ref().map_or(Status::Idle, |s| Status::Moving(*s));
                if !matches!(result, Ok(NavStatus::Searching | NavStatus::Moving | NavStatus::Working)) {
                    bot.stop_pathing();
                }
                self.arbiter.placed(worked.placed);
                self.arbiter.drove(result, worked.dug);
                status
            }
            Decision::Act(id, act) => {
                bot.stop_pathing();
                let result = act::perform(bot, &act).await;
                if result == Err(ActionError::Disconnected) {
                    return Err(ActionError::Disconnected);
                }
                self.arbiter.acted(id, &act, &result);
                Status::Acted
            }
            Decision::Hold => Status::Holding,
            Decision::Idle => Status::Idle,
            Decision::Empty => Status::Empty,
        };
        if self.arbiter.is_empty() {
            bot.stop_pathing();
            return Ok(Status::Empty);
        }
        bot.next_tick(|_, _| false).await?;
        Ok(status)
    }
}

impl Bot {
    /// Runs `procs` until every process has finished; their outcomes stay in `procs`.
    pub async fn run(&mut self, procs: &mut Processes) -> Result<(), ActionError> {
        while procs.tick(self).await? != Status::Empty {}
        Ok(())
    }
}

pub(crate) struct BotLook<'a>(pub &'a Bot);

impl Look for BotLook<'_> {
    fn solid(&self, pos: BlockPos) -> Option<bool> {
        self.0.block_at(pos).map(|(_, s)| !s.is_air() && !s.is_liquid())
    }

    fn find(&self, target: &Target) -> Option<Vec3> {
        let entities = &self.0.state.entities;
        let e = match target {
            Target::Player(name) => entities.players().find(|e| e.username.as_deref() == Some(name.as_str())),
            Target::Entity(id) => entities.get(*id),
        }?;
        let f = e.feet();
        Some([f.x, f.y, f.z])
    }

    fn block(&self, pos: BlockPos) -> Option<BlockView> {
        let (_, s) = self.0.block_at(pos)?;
        Some(BlockView::of(s))
    }

    fn count(&self, item: &str) -> u32 {
        let inv = &self.0.state.inventory;
        let items = &self.0.state.items;
        inv.main.iter().filter(|s| !s.is_empty() && items.name(s.network_id) == Some(item)).map(|s| u32::from(s.count)).sum()
    }

    fn free_slots(&self) -> u32 {
        self.0.state.inventory.main.iter().filter(|s| s.is_empty()).count() as u32
    }
}
