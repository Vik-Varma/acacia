//! Planned leaps (plan steps 4–5): found by rollout, executed live exactly as predicted (`Optimal`), and
//! refused by `Human` where only a pixel-perfect jump lands.

use acacia_physics::BlockPos;

use super::drive::{assert_arrived, assert_exact, drive, macro_nodes, macro_opts, plan, standing};
use super::grid::Grid;
use crate::pathfind::{Goal, MoveKind, MovementProfile};

pub const PROFILES: [MovementProfile; 2] = [MovementProfile::Optimal, MovementProfile::Human];

/// Floor up to x = 0, `gap` cells of void, floor again; a 5-wide lane.
pub fn gap_course(gap: i32) -> Grid {
    let g = Grid::new();
    g.fill([-12, 0, -2], [0, 0, 2], "stone").fill([gap + 1, 0, -2], [gap + 14, 0, 2], "stone");
    g
}

/// Plans from rest at `from` to `goal` and follows the plan; asserts arrival and, for `Optimal`, that
/// every macro move ran as predicted. `None` when no complete plan exists.
pub fn leap_and_follow(g: &Grid, from: [f32; 3], goal: BlockPos, profile: MovementProfile) -> Option<Vec<(MoveKind, BlockPos)>> {
    let mut m = standing(from);
    let opts = macro_opts(profile);
    let path = plan(g, &m, &Goal::Block(goal), &opts);
    if !path.complete {
        return None;
    }
    let macros = macro_nodes(&path);
    let o = drive(g, &mut m, path.nodes, &Goal::Block(goal), &opts, |_, _| {});
    eprintln!("{profile:?} to {goal:?}: {} ticks, macros {macros:?}, reports {:?}", o.ticks, o.reports);
    assert_arrived(&o, goal);
    assert_eq!(o.diverged, 0, "{profile:?}");
    if profile == MovementProfile::Optimal {
        assert_exact(&o);
    }
    Some(macros)
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
            assert_eq!(leaps.iter().filter(|(k, _)| *k == MoveKind::Parkour).count(), 1, "gap {gap}: {leaps:?}");
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

/// A plan made while sprinting at the gap starts from that momentum and runs exactly as planned.
#[test]
fn plans_a_leap_from_live_momentum() {
    let g = gap_course(3);
    let world = g.physics();
    let mut m = standing([-6.5, 1.0, 0.5]);
    (m.controls.forward, m.controls.sprint) = (1.0, true);
    while m.position().unwrap()[0] < -2.0 {
        m.tick(&world);
    }
    let goal = Goal::Block([7, 1, 0]);
    let opts = macro_opts(MovementProfile::Optimal);
    let path = plan(&g, &m, &goal, &opts);
    assert!(path.complete && path.nodes[0].mac.is_some(), "{:?}", path.nodes);
    let o = drive(&g, &mut m, path.nodes, &goal, &opts, |_, _| {});
    assert_arrived(&o, [7, 1, 0]);
    assert_exact(&o);
    assert!(o.reports.iter().all(|r| !r.resolved), "{:?}", o.reports);
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

#[test]
fn perf_gap_course() {
    let g = gap_course(3);
    let m = standing([-8.5, 1.0, 0.5]);
    for profile in PROFILES {
        let p = plan(&g, &m, &Goal::Block([10, 1, 0]), &macro_opts(profile));
        eprintln!("PERF gap course {profile:?}: visited {}, rollouts {}", p.visited, p.rollouts);
        assert!(p.complete);
    }
}
