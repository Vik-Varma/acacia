//! Half-block steps the builder meets on a wall top: from a bottom slab rim onto the full block at its corner.

use super::grid::Grid;
use super::pilot_sim::{fly, Sky};
use crate::movement::Movement;
use crate::pathfind::{Goal, GotoOpts};

#[test]
fn steps_up_from_a_bottom_slab_onto_a_full_block() {
    let g = Grid::floor(-10, 20);
    g.fill([0, 1, 0], [6, 4, 0], "cobblestone");
    g.fill_state([0, 5, 0], [5, 5, 0], "oak_slab", "minecraft:vertical_half=bottom");
    g.set_state([6, 5, 0], "oak_log", "pillar_axis=y");
    let mut m = Movement::new();
    m.start([2.5, 5.5, 0.5], 0.0, 0.0);
    let f = fly(&g, &mut m, Goal::Block([6, 6, 0]), GotoOpts::default(), Sky { horizon: None, delay: 0, max_ticks: 400 });
    assert_eq!(f.result, Ok(()), "ended at {:?}", f.end());
}
