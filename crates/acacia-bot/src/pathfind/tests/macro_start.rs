//! When a macro move starts live: only standing on the ground, as it was planned.

use acacia_physics::BlockPos;

use super::drive::{PROFILES, Style, assert_arrived, drive, macro_opts, seeds, standing};
use super::grid::Grid;
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
