//! Server-authoritative block breaking through `PlayerAuthInput` block actions (Geyser
//! `BlockBreakHandler`, Boar `ServerBreakBlockValidator`): `StartBreak`, `ContinueBreak` every tick,
//! then `ContinueBreak` + `PredictBreak` on the tick the vanilla break time runs out. Geyser counts
//! one tick of progress for each of those three, so the prediction lands exactly at full progress.

use std::time::Instant;

use acacia_client::proto::nbt::Value;
use acacia_client::proto::types::Action;
use acacia_physics::BlockPos;
use acacia_world::Tool;

use super::break_time::{break_ticks, BreakConditions};
use super::confirm::{Confirm, Expect, Verdict};
use super::geometry::{block_center, block_distance, facing_face, BLOCK_REACH};
use super::wire::{self, SwingSource};
use super::Face;
use crate::state::ItemStack;
use crate::{ActionError, Bot};

/// Tries of a break the server shows no change for.
const MAX_TRIES: u32 = 3;
/// Vanilla keeps swinging while mining; one swing lasts 6 ticks.
const SWING_EVERY: u32 = 5;
/// Bedrock enchantment ids in the item NBT `ench` list.
pub(crate) const EFFICIENCY: i16 = 15;
const AQUA_AFFINITY: i16 = 8;

impl Bot {
    /// Breaks the block at `pos` with the held item, taking the vanilla break time (see
    /// [`break_ticks`]), and waits until the server shows air there, mining again (up to 3 tries)
    /// while it shows the block unchanged past the governor's timeout. Physics bots only; the bot
    /// should stand still while mining. Dropping the future mid-break leaves the server mining:
    /// call [`Bot::abort_break`] then.
    pub async fn break_block(&mut self, pos: BlockPos) -> Result<(), ActionError> {
        if !self.has_physics() {
            return Err(ActionError::NotPossible("block breaking needs a physics bot".into()));
        }
        let (_, state) = self.block_at(pos).ok_or_else(|| ActionError::NotPossible(format!("{pos:?} is not loaded")))?;
        if state.is_air() || state.is_liquid() {
            return Err(ActionError::NotPossible(format!("nothing to break at {pos:?}")));
        }
        let name = state.name;
        let ticks = break_ticks(&state.mining, &self.break_conditions())
            .ok_or_else(|| ActionError::NotPossible(format!("{name} cannot be broken")))?;
        let eye = self.eye_position();
        let distance = block_distance(eye, pos);
        if distance > BLOCK_REACH {
            return Err(ActionError::NotPossible(format!("{name} at {pos:?} is {distance:.2} blocks away")));
        }
        let face = facing_face(eye, pos);
        self.admit_action().await?;
        let result = self.break_confirmed(pos, face, ticks).await;
        self.governor.settled();
        result.map_err(|e| match e {
            ActionError::Rejected(why) => ActionError::Rejected(format!("breaking {name} at {pos:?}: {why}")),
            e => e,
        })
    }

    /// Mines, then waits for the server's echo; re-mines while the server shows the block unchanged.
    async fn break_confirmed(&mut self, pos: BlockPos, face: Face, ticks: u32) -> Result<(), ActionError> {
        self.look_at(block_center(pos)).await?;
        let mut confirm: Option<Confirm> = None;
        loop {
            if self.mine(pos, face, ticks).await? {
                return Ok(());
            }
            let sent_at = Instant::now();
            let c = match &mut confirm {
                Some(c) => {
                    c.retried(self.governor.now(), self.governor.timeout_ticks());
                    self.governor.sent(true);
                    c
                }
                None => confirm.insert(self.confirmation(pos, Expect::Gone, MAX_TRIES)),
            };
            match self.await_confirm(pos, c, sent_at).await? {
                Verdict::Done => return Ok(()),
                Verdict::Retry => self.abort_break(pos),
                Verdict::Failed(why) => {
                    self.abort_break(pos);
                    return Err(ActionError::Rejected(why));
                }
                Verdict::Wait => unreachable!("await_confirm decides"),
            }
        }
    }

    /// Cancels breaking the block at `pos` (`AbortBreak` with the next tick).
    pub fn abort_break(&mut self, pos: BlockPos) {
        let face = facing_face(self.eye_position(), pos);
        if let Some(m) = self.movement.as_mut() {
            m.pending_actions.push(wire::block_action(Action::AbortBreak, pos, face));
        }
    }

    /// Sends the block actions for `ticks` ticks. `Ok(true)` if the block disappeared on its own
    /// before the prediction was due.
    async fn mine(&mut self, pos: BlockPos, face: Face, ticks: u32) -> Result<bool, ActionError> {
        for tick in 0..ticks {
            let step = if tick == 0 { Action::StartBreak } else { Action::ContinueBreak };
            let mut actions = vec![wire::block_action(step, pos, face)];
            if tick + 1 == ticks {
                actions.push(wire::block_action(Action::PredictBreak, pos, face));
            }
            if let Some(m) = self.movement.as_mut() {
                m.pending_actions.extend(actions);
            }
            if tick % SWING_EVERY == 0 {
                self.swing_from(Some(SwingSource::Mine));
            }
            self.next_tick(|_, _| false).await?;
            if tick + 1 < ticks && self.is_air(pos) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn is_air(&self, pos: BlockPos) -> bool {
        self.block_at(pos).is_some_and(|(_, s)| s.is_air())
    }

    /// Break-time inputs from the tracked state. Haste and Mining Fatigue are not tracked yet.
    pub fn break_conditions(&self) -> BreakConditions {
        let inv = &self.state.inventory;
        let held = inv.held();
        let eye = self.eye_position();
        let eye_block = [eye[0].floor() as i32, eye[1].floor() as i32, eye[2].floor() as i32];
        let helmet = inv.armor.first().cloned().unwrap_or_default();
        BreakConditions {
            tool: self.state.items.name(held.network_id).and_then(Tool::from_identifier),
            efficiency: enchantment_level(held, EFFICIENCY),
            haste: 0,
            mining_fatigue: 0,
            underwater: self.block_at(eye_block).is_some_and(|(_, s)| s.is_water()) && enchantment_level(&helmet, AQUA_AFFINITY) == 0,
            // Non-physics bots can't break blocks; true keeps the formula's default.
            on_ground: self.movement.as_ref().is_none_or(|m| m.on_ground()),
        }
    }
}

/// Level of enchantment `id` in the item's NBT `ench` list, 0 if absent.
pub(crate) fn enchantment_level(item: &ItemStack, id: i16) -> u8 {
    let Some(Value::List(list)) = item.nbt.as_ref().and_then(|n| n.value.get("ench")) else { return 0 };
    list.items
        .iter()
        .find(|e| matches!(e.get("id"), Some(Value::Short(i)) if *i == id))
        .and_then(|e| match e.get("lvl") {
            Some(Value::Short(l)) => u8::try_from(*l).ok(),
            _ => None,
        })
        .unwrap_or(0)
}
