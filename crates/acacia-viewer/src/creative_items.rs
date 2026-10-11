//! The creative inventory's items as the screen lists them.

use acacia_bot::Bot;

use crate::control::{Stack, stack_of};

/// One entry of the creative inventory: its id, the item, and its tab (an index into acacia-ui's
/// `TABS`).
#[derive(Debug, Clone, PartialEq)]
pub struct CreativeEntry {
    pub id: u32,
    pub stack: Stack,
    pub tab: usize,
}

pub fn creative(bot: &Bot) -> Vec<CreativeEntry> {
    let entry = |i: &acacia_bot::state::CreativeItem| Some(CreativeEntry { id: i.entry_id, stack: stack_of(bot, &i.stack)?, tab: i.tab as usize });
    bot.state().creative.items().iter().filter_map(entry).collect()
}
