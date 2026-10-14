//! Leaps that change height or go diagonally (plan step 5), and the leaps that must not be planned.

use acacia_physics::BlockPos;

use super::drive::{macro_opts, plan, standing};
use super::grid::Grid;
use super::leaps::{PROFILES, gap_course, leap_and_follow};
use crate::pathfind::{Goal, MoveKind, MovementProfile};

const START: [f32; 3] = [-0.5, 1.0, 0.5];

fn has_leap_to(leaps: &[(MoveKind, BlockPos)], to: impl Fn(BlockPos) -> bool) -> bool {
    leaps.iter().any(|&(k, p)| k == MoveKind::Parkour && to(p))
}

#[test]
fn leaps_up_one_block() {
    let g = Grid::new();
    g.fill([-12, 0, -2], [0, 0, 2], "stone").fill([3, 0, -2], [16, 1, 2], "stone");
    for profile in PROFILES {
        let leaps = leap_and_follow(&g, START, [7, 2, 0], profile).expect("plan");
        assert!(has_leap_to(&leaps, |p| p[1] == 2), "{profile:?}: {leaps:?}");
    }
}

#[test]
fn leaps_down_two_blocks() {
    let g = Grid::new();
    g.fill([-12, 0, -2], [0, 2, 2], "stone").fill([3, 0, -2], [16, 0, 2], "stone");
    for profile in PROFILES {
        let leaps = leap_and_follow(&g, [-0.5, 3.0, 0.5], [7, 1, 0], profile).expect("plan");
        assert!(has_leap_to(&leaps, |p| p[1] == 1), "{profile:?}: {leaps:?}");
    }
}

/// Six blocks down is too far onto stone, not into water.
#[test]
fn leaps_down_into_water() {
    let g = Grid::new();
    g.fill([-12, 0, -2], [0, 5, 2], "stone").fill([2, -2, -3], [14, -2, 3], "stone");
    g.fill_state([2, -1, -2], [8, 0, 2], "water", "liquid_depth=0").fill([9, -1, -2], [14, 0, 2], "stone");
    for profile in PROFILES {
        let mut m = standing([-0.5, 6.0, 0.5]);
        let path = plan(&g, &m, &Goal::Block([12, 1, 0]), &macro_opts(profile));
        assert!(path.complete, "{profile:?}");
        assert!(path.nodes.iter().any(|n| n.kind == MoveKind::Parkour && n.wet), "{profile:?}: {:?}", path.nodes);
        let o = super::drive::drive(&g, &mut m, path.nodes, &Goal::Block([12, 1, 0]), &macro_opts(profile), |_, _| {});
        super::drive::assert_arrived(&o, [12, 1, 0]);
        if profile == MovementProfile::Optimal {
            super::drive::assert_exact(&o);
        }
    }
}

/// A one-block target off the diagonal, and a long walkway around for whoever will not leap.
fn diagonal_course((a, b): (i32, i32)) -> Grid {
    let g = Grid::new();
    g.fill([-6, 0, -3], [0, 0, 0], "stone").fill([a, 0, b], [a, 0, b], "stone");
    g.fill([1, 0, -3], [a + 2, 0, -3], "stone").fill([a + 2, 0, -3], [a + 2, 0, b], "stone").fill([a + 1, 0, b], [a + 1, 0, b], "stone");
    g
}

#[test]
fn leaps_diagonally() {
    for (a, b) in [(2, 1), (3, 1), (3, 2)] {
        let g = diagonal_course((a, b));
        for profile in PROFILES {
            let leaps = leap_and_follow(&g, START, [a, 1, b], profile).expect("plan");
            eprintln!("diagonal ({a},{b}) {profile:?}: {leaps:?}");
            if profile == MovementProfile::Optimal {
                assert!(has_leap_to(&leaps, |p| p == [a, 1, b]), "({a},{b}): {leaps:?}");
            }
        }
    }
}

/// A ceiling two blocks up cuts every jump short: no leap is planned under it.
#[test]
fn rejects_head_hitter_leaps() {
    let g = gap_course(2);
    g.fill([-12, 3, -2], [16, 3, 2], "stone");
    for profile in PROFILES {
        let path = plan(&g, &standing(START), &Goal::Block([6, 1, 0]), &macro_opts(profile));
        assert!(!path.complete && path.nodes.iter().all(|n| n.kind != MoveKind::Parkour), "{profile:?}: {:?}", path.nodes);
    }
}

/// Lava filling a gap is cleared; a landing lane with lava beside it is too close for `Human`.
#[test]
fn lava_gap_keeps_a_margin() {
    let g = gap_course(3);
    g.fill_state([1, 0, -2], [3, 0, 2], "lava", "liquid_depth=0");
    for profile in PROFILES {
        let leaps = leap_and_follow(&g, START, [7, 1, 0], profile).expect("plan");
        assert!(has_leap_to(&leaps, |p| p[0] >= 4), "{profile:?}: {leaps:?}");
    }
    let g = gap_course(3);
    g.fill_state([1, 0, -2], [3, 0, 2], "lava", "liquid_depth=0");
    g.fill_state([4, 1, -2], [16, 1, -1], "lava", "liquid_depth=0").fill_state([4, 1, 1], [16, 1, 2], "lava", "liquid_depth=0");
    let leaps = leap_and_follow(&g, START, [7, 1, 0], MovementProfile::Optimal).expect("optimal plan");
    assert!(has_leap_to(&leaps, |p| p[0] >= 4), "{leaps:?}");
    let human = plan(&g, &standing(START), &Goal::Block([7, 1, 0]), &macro_opts(MovementProfile::Human));
    assert!(!human.complete, "{:?}", human.nodes);
}
