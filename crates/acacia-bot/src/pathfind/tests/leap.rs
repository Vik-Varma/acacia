//! Macro moves executed live: a hand-built leap runs exactly as predicted, and a knock off its course
//! diverges into a re-plan without the bot stopping.

use acacia_client::proto::packets::SetEntityMotion;
use acacia_client::proto::types::Vec3f;
use acacia_physics::{BlockPos, Vec3};

use super::drive::{Outcome, PROFILES, Style, assert_arrived, assert_exact, drive, macro_opts, seeds, standing};
use super::grid::Grid;
use crate::movement::Movement;
use crate::pathfind::{Aim, Entry, Goal, JumpAt, LeapCtl, Macro, Momentum, MoveKind, MovementProfile, PathNode, ROLLOUT_BUDGET, Work, run};
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

/// Follows the hand-built leap path under every execution seed of `profile`.
fn follow(g: &Grid, profile: MovementProfile, mut nudge: impl FnMut(u32, &mut Movement)) -> Vec<Outcome> {
    let path = leap_path(g, profile);
    let opts = macro_opts(profile);
    seeds(profile).iter().map(|&seed| drive(g, &mut standing(START), path.clone(), &Goal::Block(GOAL), &opts, Style { seed, shortcuts: false }, &mut nudge)).collect()
}

fn knock(m: &mut Movement, x: f32, z: f32) {
    let knock = SetEntityMotion { runtime_entity_id: 1, velocity: Vec3f { x, y: 0.0, z }, tick: 0 };
    m.apply(&raw(&knock), &ME).unwrap();
}

#[test]
fn a_leap_executes_exactly_as_predicted() {
    let g = gap();
    for profile in PROFILES {
        for o in follow(&g, profile, |_, _| {}) {
            assert_arrived(&o, GOAL);
            assert_eq!(o.diverged, 0, "{profile:?}");
            assert_eq!(o.reports.len(), 1, "{profile:?}: {:?}", o.reports);
            assert_exact(&o);
        }
    }
}

#[test]
fn a_knock_mid_leap_diverges_and_replans_without_stopping() {
    let g = gap();
    for profile in PROFILES {
        let mut airborne = 0;
        let runs = follow(&g, profile, |t, m| {
            airborne = if t == 0 { 0 } else { airborne + u32::from(!m.on_ground()) };
            if airborne == 3 {
                airborne += 1;
                knock(m, -0.5, 0.0);
            }
        });
        for o in runs {
            assert_arrived(&o, GOAL);
            assert!(o.diverged >= 1 && o.held, "{profile:?}: diverged {} times, held {}", o.diverged, o.held);
        }
    }
}

/// A sideways shove just before takeoff: the leap is solved again live within the per-tick budget (or the
/// bot re-plans without stopping), and lands.
#[test]
fn a_drifting_leap_resolves_within_the_tick_budget() {
    let g = gap();
    for profile in PROFILES {
        for o in follow(&g, profile, |t, m| {
            if t == 4 {
                knock(m, 0.0, 0.12);
            }
        }) {
            eprintln!("{profile:?}: {} ticks, diverged {}, max spend {}, reports {:?}", o.ticks, o.diverged, o.max_spend, o.reports);
            assert_arrived(&o, GOAL);
            assert!(o.held && o.max_spend <= ROLLOUT_BUDGET, "{profile:?}");
            assert!(o.diverged > 0 || o.reports.iter().any(|r| r.resolved || r.repredicted > 0), "{profile:?}: {:?}", o.reports);
        }
    }
}
