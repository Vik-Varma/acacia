//! Macro moves executed live: a hand-built leap runs exactly as predicted, and a knock off its course
//! diverges into a re-plan without the bot stopping.

use acacia_client::proto::packets::SetEntityMotion;
use acacia_client::proto::types::Vec3f;
use acacia_physics::{BlockPos, Vec3};

use super::drive::{Outcome, assert_arrived, assert_exact, drive, macro_opts, standing};
use super::grid::Grid;
use crate::movement::Movement;
use crate::pathfind::{Aim, Entry, Goal, JumpAt, LeapCtl, Macro, Momentum, MoveKind, MovementProfile, PathNode, Work, run};
use crate::state::Me;
use crate::state::queries::test_support::raw;

const ME: Me = Me { runtime_entity_id: 1, unique_entity_id: -1 };
const START: Vec3 = [-1.5, 1.0, 0.5];
const LAND: BlockPos = [4, 1, 0];
const GOAL: BlockPos = [6, 1, 0];

/// Floor up to x = 0, a three-block gap over a one-deep pit, floor again from x = 4.
fn gap() -> Grid {
    let g = Grid::new();
    g.fill([-5, 0, -2], [0, 0, 2], "stone").fill([4, 0, -2], [9, 0, 2], "stone").fill([1, -1, -2], [3, -1, 2], "stone");
    g
}

fn node(pos: BlockPos, kind: MoveKind, mac: Option<Macro>) -> PathNode {
    PathNode { pos, feet: pos[1] as f32, kind, wet: false, work: Work::default(), entry: Entry::Rest, mom: Momentum::rest(pos[1] as f32, pos), mac }
}

/// A leap from the run-up over the gap, then walking on to the goal, with the coast picked by hand.
fn leap_path(g: &Grid, profile: MovementProfile) -> Vec<PathNode> {
    let m = standing(START);
    let leap = [None].into_iter().chain((1..12).map(Some)).find_map(|coast| {
        let leap = LeapCtl::new(LAND, 1.0, JumpAt::Edge, coast, Aim::new(profile, 7));
        run(&mut m.fork().unwrap(), &mut { leap }, m.controls, 60, &g.terrain()).landed.then_some(leap)
    });
    let leap = leap.unwrap_or_else(|| panic!("{profile:?}: no coast lands the leap"));
    vec![node(LAND, MoveKind::Parkour, Some(Macro::Leap(leap))), node([5, 1, 0], MoveKind::Walk, None), node(GOAL, MoveKind::Walk, None)]
}

fn follow(g: &Grid, profile: MovementProfile, nudge: impl FnMut(u32, &mut Movement)) -> Outcome {
    let mut m = standing(START);
    drive(g, &mut m, leap_path(g, profile), &Goal::Block(GOAL), &macro_opts(profile), nudge)
}

#[test]
fn a_leap_executes_exactly_as_predicted() {
    let g = gap();
    for profile in [MovementProfile::Optimal, MovementProfile::Human] {
        let o = follow(&g, profile, |_, _| {});
        assert_arrived(&o, GOAL);
        assert_eq!(o.diverged, 0, "{profile:?}");
        assert_eq!(o.reports.len(), 1, "{profile:?}: {:?}", o.reports);
        assert_exact(&o);
    }
}

#[test]
fn a_knock_mid_leap_diverges_and_replans_without_stopping() {
    let g = gap();
    let mut airborne = 0;
    let o = follow(&g, MovementProfile::Optimal, |_, m| {
        if !m.on_ground() {
            airborne += 1;
        }
        if airborne == 3 {
            airborne += 1;
            let knock = SetEntityMotion { runtime_entity_id: 1, velocity: Vec3f { x: -0.5, y: 0.0, z: 0.0 }, tick: 0 };
            m.apply(&raw(&knock), &ME).unwrap();
        }
    });
    assert_arrived(&o, GOAL);
    assert!(o.diverged >= 1 && o.held, "diverged {} times, held {}", o.diverged, o.held);
}
