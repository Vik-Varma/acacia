//! Any-angle shortcuts while following (plan step 6b): straight lines instead of the 8-direction staircase.

use super::drive::{PROFILES, macro_opts, plan_and_follow};
use super::grid::Grid;
use crate::pathfind::SearchOpts;

/// Ticks to [15, 1, 6] under every execution seed.
fn follow(g: &Grid, opts: &SearchOpts, shortcuts: bool) -> Vec<u32> {
    let (_, runs) = plan_and_follow(g, [-15.5, 1.0, -5.5], [15, 1, 6], opts, shortcuts).expect("plan");
    runs.iter().map(|o| o.ticks).collect()
}

/// 30 blocks east and 11 north: the plan staircases, the shortcut runs the line.
#[test]
fn shortcuts_cut_a_long_diagonal() {
    let g = Grid::floor(-31, 46);
    for profile in PROFILES {
        for macros in [false, true] {
            let opts = SearchOpts { macros, ..macro_opts(profile) };
            let (without, with) = (follow(&g, &opts, false), follow(&g, &opts, true));
            eprintln!("{profile:?}, macros {macros}: {with:?} ticks with shortcuts, {without:?} without");
            assert!(with.iter().zip(&without).all(|(w, o)| w < o), "{profile:?}, macros {macros}: {with:?} vs {without:?}");
        }
    }
}

/// A wall across the line: no shortcut through it, the path around still arrives.
#[test]
fn shortcuts_keep_out_of_walls() {
    let g = Grid::floor(-31, 46);
    g.fill([0, 1, -12], [0, 3, 3], "stone");
    for profile in PROFILES {
        follow(&g, &macro_opts(profile), true);
    }
}
