//! Macro moves start live as they were planned: standing on the ground, and from the bot's real facing.

use acacia_physics::BlockPos;

use super::drive::{PROFILES, Style, assert_arrived, drive, macro_opts, plan, seeds, standing};
use super::grid::Grid;
use crate::movement::Movement;
use crate::pathfind::{Aim, Entry, Goal, Macro, Momentum, MoveKind, PathNode, RunCtl, Work};

fn node(pos: BlockPos, kind: MoveKind, mac: Option<Macro>) -> PathNode {
    PathNode { pos, feet: pos[1] as f32, kind, wet: false, work: Work::default(), entry: Entry::Rest, mom: Momentum::rest(pos[1] as f32, pos), mac, cost: 0.0 }
}

/// A step up (classic Ascend, reached while still rising) and then a planned run along the top: the run waits for the
/// landing instead of failing in the air (`Optimal` diverged there on the speed test's stairs).
#[test]
fn a_run_after_a_step_up_starts_on_landing() {
    let g = Grid::floor(-10, 20);
    g.fill([3, 1, -2], [20, 1, 2], "stone");
    for profile in PROFILES {
        let run = RunCtl::new([5, 2, 0], 2.0, Aim::new(profile, 7));
        let mut path = vec![node([1, 1, 0], MoveKind::Walk, None), node([2, 1, 0], MoveKind::Walk, None), node([3, 2, 0], MoveKind::Ascend, None)];
        path.push(node([5, 2, 0], MoveKind::Run, Some(Macro::Run(run))));
        path.extend([6, 7, 8].map(|x| node([x, 2, 0], MoveKind::Walk, None)));
        let goal = Goal::Block([8, 2, 0]);
        for &seed in seeds(profile) {
            let o = drive(&g, &mut standing([0.5, 1.0, 0.5]), path.clone(), &goal, &macro_opts(profile), Style { seed, shortcuts: false }, |_, _| {});
            assert_arrived(&o, [8, 2, 0]);
            assert_eq!(o.diverged, 0, "{profile:?} seed {seed}");
        }
    }
}

/// A plan made standing still with the bot facing back or sideways: a leap from the start is planned from that yaw
/// (rest nodes elsewhere are planned facing their leap), so it runs as planned.
#[test]
fn a_leap_from_the_start_plans_from_the_real_facing() {
    // A two-gap up onto a step (the course's up1): a leap the follower cannot re-solve from a backward start.
    let g = Grid::new();
    g.fill([-12, 0, -2], [0, 0, 2], "stone").fill([3, 0, -2], [16, 1, 2], "stone");
    let goal = Goal::Block([7, 2, 0]);
    for profile in PROFILES {
        let opts = macro_opts(profile);
        for yaw in [90.0, 0.0, 180.0] {
            // Settled on the ground first: a search from a fresh (airborne) state plans no macro move at its start.
            let facing = |yaw: f32| {
                let mut m = Movement::new();
                m.start([-0.5, 1.0, 0.5], yaw, 0.0);
                m.controls.yaw = yaw;
                for _ in 0..5 {
                    m.tick(&g.physics());
                }
                m
            };
            let path = plan(&g, &facing(yaw), &goal, &opts);
            assert!(path.complete, "{profile:?} yaw {yaw}: no plan");
            for &seed in seeds(profile) {
                let o = drive(&g, &mut facing(yaw), path.nodes.clone(), &goal, &opts, Style { seed, shortcuts: true }, |_, _| {});
                eprintln!("FACING {profile:?} yaw {yaw} seed {seed}: {} ticks, {:?}", o.ticks, o.reports);
                assert_arrived(&o, [7, 2, 0]);
                // The first move (a macro from the start) runs as planned: no live re-solve, no divergence.
                assert_eq!(o.diverged, 0, "{profile:?} yaw {yaw} seed {seed}: {:?}", o.reports);
                assert!(o.reports.first().is_some_and(|r| !r.resolved), "{profile:?} yaw {yaw} seed {seed}: {:?}", o.reports);
            }
        }
    }
}
