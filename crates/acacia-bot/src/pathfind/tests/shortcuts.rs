//! Any-angle shortcuts while following (plan step 6b): straight lines instead of the 8-direction staircase.

use super::drive::{Outcome, assert_arrived, assert_exact, drive_with, macro_opts, plan, standing};
use super::grid::Grid;
use super::leaps::PROFILES;
use crate::pathfind::{Goal, MovementProfile, SearchOpts};

fn follow(g: &Grid, goal: [i32; 3], opts: &SearchOpts, shortcuts: bool) -> Outcome {
    let mut m = standing([-15.5, 1.0, -5.5]);
    let p = plan(g, &m, &Goal::Block(goal), opts);
    assert!(p.complete);
    let o = drive_with(g, &mut m, p.nodes, &Goal::Block(goal), opts, shortcuts, |_, _| {});
    assert_arrived(&o, goal);
    assert_eq!(o.diverged, 0, "{:?}", opts.profile);
    if opts.profile == MovementProfile::Optimal {
        assert_exact(&o);
    }
    o
}

/// 30 blocks east and 11 north: the plan staircases, the shortcut runs the line.
#[test]
fn shortcuts_cut_a_long_diagonal() {
    let g = Grid::floor(-31, 46);
    for profile in PROFILES {
        for macros in [false, true] {
            let opts = SearchOpts { macros, ..macro_opts(profile) };
            let without = follow(&g, [15, 1, 6], &opts, false).ticks;
            let with = follow(&g, [15, 1, 6], &opts, true).ticks;
            eprintln!("{profile:?}, macros {macros}: {with} ticks with shortcuts, {without} without");
            assert!(with < without, "{profile:?}, macros {macros}: {with} vs {without}");
        }
    }
}

/// A wall across the line: no shortcut through it, the path around still arrives.
#[test]
fn shortcuts_keep_out_of_walls() {
    let g = Grid::floor(-31, 46);
    g.fill([0, 1, -12], [0, 3, 3], "stone");
    let opts = macro_opts(MovementProfile::Optimal);
    follow(&g, [15, 1, 6], &opts, true);
}
