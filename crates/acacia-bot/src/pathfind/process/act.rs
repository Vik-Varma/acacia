//! Does an [`Act`] with the bot's own actions.

use acacia_physics::BlockPos;

use super::Act;
use crate::interact::{block_distance, Face, BLOCK_REACH};
use crate::survival::Destination;
use crate::{ActionError, Bot};

/// Faces to click, in preference order: placing on top of the block below first.
const AGAINST: [Face; 6] = [Face::Up, Face::North, Face::South, Face::West, Face::East, Face::Down];

pub(super) async fn perform(bot: &mut Bot, act: &Act) -> Result<(), ActionError> {
    match act {
        Act::Fill { pos, blocks } => fill(bot, *pos, blocks).await,
        Act::Place { item, against, face, click } => {
            hold(bot, item).await?;
            bot.place_block_at(*against, *face, *click).await
        }
        Act::Break { pos } => {
            if bot.block_at(*pos).is_some_and(|(_, s)| s.is_air() || s.is_liquid()) {
                return Ok(());
            }
            bot.equip_best_tool(*pos).await?;
            bot.break_block(*pos).await
        }
        Act::Restock { station, items } => restock(bot, *station, items).await,
    }
}

async fn hold(bot: &mut Bot, item: &str) -> Result<(), ActionError> {
    let slot = bot.find_item(item).ok_or_else(|| ActionError::NotPossible(format!("no {item} to place")))?;
    bot.equip(slot, Destination::Hand).await
}

/// Places the first of `blocks` held into `pos`; done once the server shows a block there.
async fn fill(bot: &mut Bot, pos: BlockPos, blocks: &[String]) -> Result<(), ActionError> {
    let solid = |bot: &Bot, p: BlockPos| bot.block_at(p).map(|(_, s)| !s.is_air() && !s.is_liquid());
    match solid(bot, pos) {
        Some(true) => return Ok(()),
        None => return Err(ActionError::NotPossible(format!("{pos:?} is not loaded"))),
        Some(false) => {}
    }
    let eye = bot.eye_position();
    let face = AGAINST.into_iter().find(|f| {
        let against = against(pos, *f);
        solid(bot, against) == Some(true) && block_distance(eye, against) <= BLOCK_REACH
    });
    let face = face.ok_or_else(|| ActionError::NotPossible(format!("nothing to place against around {pos:?} within reach")))?;
    let slot = blocks.iter().find_map(|b| bot.find_item(b));
    let slot = slot.ok_or_else(|| ActionError::NotPossible(format!("none of {blocks:?} to fill {pos:?} with")))?;
    bot.equip(slot, Destination::Hand).await?;
    bot.place_block(against(pos, face), face).await
}

/// Takes whole stacks of the wanted items until each count is covered or the inventory is full.
async fn restock(bot: &mut Bot, station: BlockPos, items: &[(String, u32)]) -> Result<(), ActionError> {
    let container = bot.open_container_at(station).await?;
    let mut want: Vec<(String, u32)> = items.to_vec();
    let mut result = Ok(());
    for (slot, stack) in container.slots.iter().enumerate() {
        let Some(name) = bot.state.items.name(stack.network_id).map(str::to_string) else { continue };
        let Some(w) = want.iter_mut().find(|(n, c)| *n == name && *c > 0) else { continue };
        let Ok(slot) = u8::try_from(slot) else { continue };
        match bot.move_to_inventory(slot).await {
            Ok(_) => w.1 = w.1.saturating_sub(u32::from(stack.count)),
            Err(ActionError::NotPossible(_)) => break,
            Err(e) => {
                result = Err(e);
                break;
            }
        }
    }
    let closed = bot.close_container().await;
    result.and(closed)
}

/// The block whose `face` touches `pos`.
fn against(pos: BlockPos, face: Face) -> BlockPos {
    let [dx, dy, dz] = face.offset();
    [pos[0] - dx, pos[1] - dy, pos[2] - dz]
}
