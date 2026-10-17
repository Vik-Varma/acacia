//! Does an [`Act`] with the bot's own actions.

use acacia_physics::BlockPos;

use super::Act;
use crate::interact::{block_distance, Face, BLOCK_REACH};
use crate::survival::Destination;
use crate::{ActionError, Bot};

/// Faces to click, in preference order: placing on top of the block below first.
const AGAINST: [Face; 6] = [Face::Up, Face::North, Face::South, Face::West, Face::East, Face::Down];

pub(super) async fn perform(bot: &mut Bot, act: &Act) -> Result<(), ActionError> {
    let Act::Fill { pos, blocks } = act;
    fill(bot, *pos, blocks).await
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

/// The block whose `face` touches `pos`.
fn against(pos: BlockPos, face: Face) -> BlockPos {
    let [dx, dy, dz] = face.offset();
    [pos[0] - dx, pos[1] - dy, pos[2] - dz]
}
