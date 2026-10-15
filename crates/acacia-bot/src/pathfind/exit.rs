//! Dead ends for partial paths: a node the bot cannot leave (a pit) must not end one.

use std::collections::VecDeque;

use acacia_physics::BlockPos;

use super::moves::{self, Ctx};
use super::node::{Entry, Momentum};
use super::opts::SearchOpts;
use super::terrain::{Blocks, PosMap, Spot, Terrain};

/// Cells a classic flood from a node must reach for it to count as open; a 3×3 pit has 9.
pub(crate) const EXIT_NODES: usize = 64;

/// Classic moves (with the search's dig, bridge and door options) from `pos` reach [`EXIT_NODES`] cells.
/// `placed`: scaffold blocks the path used getting there.
pub(crate) fn has_exit<B: Blocks>(t: &Terrain<B>, pos: BlockPos, spot: Spot, opts: &SearchOpts, placed: u32) -> bool {
    let blocks_left = opts.kit.blocks.saturating_sub(placed);
    let ctx = Ctx { parkour: opts.parkour, dig: opts.allow_dig, bridge: opts.allow_bridge, doors: opts.open_doors, kit: &opts.kit, blocks_left, macros: None };
    let mut seen = PosMap::default();
    seen.insert(pos, ());
    let (mut queue, mut edges, rest) = (VecDeque::from([(pos, spot)]), Vec::new(), Momentum::default());
    while let Some((p, s)) = queue.pop_front() {
        edges.clear();
        moves::successors(t, p, s, (Entry::Rest, &rest), &ctx, &mut edges);
        for e in &edges {
            if seen.insert(e.pos, ()).is_none() {
                if seen.len() >= EXIT_NODES {
                    return true;
                }
                queue.push_back((e.pos, e.spot));
            }
        }
    }
    false
}
