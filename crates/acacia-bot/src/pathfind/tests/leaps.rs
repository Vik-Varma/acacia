//! Planned leaps (plan steps 4–5): found by rollout, executed live exactly as predicted (`Optimal`) or under
//! every `Human` aim seed, and refused by `Human` where only a pixel-perfect jump lands.

use acacia_physics::BlockPos;

use super::drive::{PROFILES, Style, assert_arrived, assert_exact, drive, macro_nodes, macro_opts, plan, plan_and_follow, seeds, standing};
use super::grid::Grid;
use crate::movement::Movement;
use crate::pathfind::{Goal, MoveKind, MovementProfile};

/// Floor up to x = 0, `gap` cells of void, floor again; a 5-wide lane.
pub fn gap_course(gap: i32) -> Grid {
    let g = Grid::new();
    g.fill([-12, 0, -2], [0, 0, 2], "stone").fill([gap + 1, 0, -2], [gap + 14, 0, 2], "stone");
    g
}

/// Plans from rest at `from` to `goal` and follows the plan ([`plan_and_follow`]): the macro moves planned,
/// `None` when no complete plan exists.
pub fn leap_and_follow(g: &Grid, from: [f32; 3], goal: BlockPos, profile: MovementProfile) -> Option<Vec<(MoveKind, BlockPos)>> {
    plan_and_follow(g, from, goal, &macro_opts(profile), false).map(|(p, _)| macro_nodes(&p))
}

/// Gaps 1–4 from `from`: one leap each; `Human` refuses the four-gap, which only a pixel-perfect jump lands.
fn leaps_gaps_from(from: [f32; 3]) {
    for gap in 1..=4 {
        let g = gap_course(gap);
        for profile in PROFILES {
            let leaps = leap_and_follow(&g, from, [gap + 4, 1, 0], profile);
            eprintln!("from {from:?}, gap {gap}, {profile:?}: {leaps:?}");
            if profile == MovementProfile::Human && gap == 4 {
                assert!(leaps.is_none(), "{leaps:?}");
                continue;
            }
            let leaps = leaps.unwrap_or_else(|| panic!("gap {gap}, {profile:?}: no plan"));
            // One jump over the gap: a leap, or a hop from a run (they tie, and lazily solved leaps come second).
            let jumps = |w: &[(MoveKind, [i32; 3])]| matches!(w[1].0, MoveKind::Parkour | MoveKind::Hop) && w[0].1[0] <= 0 && w[1].1[0] > gap;
            let first = [(MoveKind::Walk, [from[0].floor() as i32, 1, 0])];
            let crossings = first.iter().chain(&leaps).copied().collect::<Vec<_>>().windows(2).filter(|w| jumps(w)).count();
            assert_eq!(crossings, 1, "gap {gap}: {leaps:?}");
        }
    }
}

#[test]
fn leaps_gaps_of_one_to_four_from_rest() {
    leaps_gaps_from([0.5, 1.0, 0.5]);
}

#[test]
fn leaps_gaps_after_a_run_up() {
    leaps_gaps_from([-5.5, 1.0, 0.5]);
}

/// Sprinting at a three-gap from x = -6.5, stopped at x = -2.
fn sprinting(g: &Grid) -> Movement {
    let world = g.physics();
    let mut m = standing([-6.5, 1.0, 0.5]);
    (m.controls.forward, m.controls.sprint) = (1.0, true);
    while m.position().unwrap()[0] < -2.0 {
        m.tick(&world);
    }
    m
}

/// A plan made while sprinting at the gap starts from that momentum and runs as planned (`Optimal` exactly).
#[test]
fn plans_a_leap_from_live_momentum() {
    let g = gap_course(3);
    let goal = Goal::Block([7, 1, 0]);
    for profile in PROFILES {
        let opts = macro_opts(profile);
        let path = plan(&g, &sprinting(&g), &goal, &opts);
        let standing_there = plan(&g, &standing(sprinting(&g).position().unwrap()), &goal, &opts);
        assert!(path.complete && path.cost <= standing_there.cost, "{profile:?}: {} vs {}: {:?}", path.cost, standing_there.cost, path.nodes);
        for &seed in seeds(profile) {
            let o = drive(&g, &mut sprinting(&g), path.nodes.clone(), &goal, &opts, Style { seed, shortcuts: false }, |_, _| {});
            assert_arrived(&o, [7, 1, 0]);
            assert_eq!(o.diverged, 0, "{profile:?} seed {seed}");
            if profile == MovementProfile::Optimal {
                assert_exact(&o);
                assert!(o.reports.iter().all(|r| !r.resolved), "{:?}", o.reports);
            }
        }
    }
}

/// A four-gap next to a one-deep pit: `Optimal` leaps it, `Human` walks through the pit.
#[test]
fn human_detours_around_a_pixel_perfect_gap() {
    let g = gap_course(4);
    g.fill([1, -1, -2], [4, -1, 2], "stone");
    let goal = [8, 1, 0];
    let optimal = leap_and_follow(&g, [0.5, 1.0, 0.5], goal, MovementProfile::Optimal).expect("optimal plan");
    assert!(optimal.iter().any(|(k, _)| *k == MoveKind::Parkour), "{optimal:?}");
    let human = leap_and_follow(&g, [0.5, 1.0, 0.5], goal, MovementProfile::Human).expect("human plan");
    assert!(human.iter().all(|(k, p)| *k != MoveKind::Parkour || p[0] < 5), "{human:?}");
}
