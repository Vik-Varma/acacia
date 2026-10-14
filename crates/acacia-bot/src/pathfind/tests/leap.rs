//! Macro moves executed live: a hand-built leap runs exactly as predicted, and a knock off its course
//! diverges into a re-plan without the bot stopping.

use acacia_client::proto::packets::SetEntityMotion;
use acacia_client::proto::types::Vec3f;
use acacia_physics::{BlockPos, Vec3};

use super::grid::Grid;
use super::search::opts;
use super::sim::sense;
use crate::movement::Movement;
use crate::pathfind::{Aim, Entry, FollowStatus, Follower, Goal, JumpAt, LeapCtl, Macro, MacroReport, MoveKind, MovementProfile, PathNode, Work, run, search};
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
    PathNode { pos, feet: pos[1] as f32, kind, wet: false, work: Work::default(), entry: Entry::Rest, mac }
}

/// A leap from the run-up over the gap, then walking on to the goal. A full sprint jump overshoots the
/// landing cell, so the coast is picked by rollout (the planner's timing search stands in here).
fn leap_path(g: &Grid, profile: MovementProfile) -> Vec<PathNode> {
    let mut m = Movement::new();
    m.start(START, -90.0, 0.0);
    let leap = [None].into_iter().chain((1..12).map(Some)).find_map(|coast| {
        let leap = LeapCtl::new(LAND, 1.0, JumpAt::Edge, coast, Aim::new(profile, 7));
        run(&mut m.fork().unwrap(), &mut { leap }, m.controls, 60, &g.terrain()).landed.then_some(leap)
    });
    let leap = leap.unwrap_or_else(|| panic!("{profile:?}: no coast lands the leap"));
    vec![node(LAND, MoveKind::Parkour, Some(Macro::Leap(leap))), node([5, 1, 0], MoveKind::Walk, None), node(GOAL, MoveKind::Walk, None)]
}

struct Outcome {
    reports: Vec<MacroReport>,
    diverged: u32,
    /// Forward was still held on every tick that diverged.
    held: bool,
    end: Vec3,
}

/// Follows `path`, re-planning inline on divergence as the navigator does; `nudge` may disturb the bot.
fn drive(g: &Grid, path: Vec<PathNode>, mut nudge: impl FnMut(u32, &mut Movement)) -> Outcome {
    let world = g.physics();
    let mut m = Movement::new();
    m.start(START, -90.0, 0.0);
    let mut follower = Follower::new(path, START, 0.25);
    let mut out = Outcome { reports: Vec::new(), diverged: 0, held: true, end: START };
    for tick in 0..300 {
        nudge(tick, &mut m);
        let s = sense(g, &m);
        let live = m.fork();
        let status = follower.tick(&s, live.as_ref(), &g.terrain(), &mut m.controls);
        out.reports.extend(follower.take_macro_report());
        match status {
            FollowStatus::Moving => {}
            FollowStatus::Arrived => {
                out.end = s.pos;
                return out;
            }
            FollowStatus::Diverged => {
                eprintln!("tick {tick}: diverged at {:?}, controls {:?}", s.pos, m.controls);
                out.diverged += 1;
                out.held &= m.controls.forward != 0.0;
                let path = search(&g.terrain(), s.pos, &Goal::Block(GOAL), &opts(true));
                assert!(path.complete, "no path from {:?}", s.pos);
                follower = Follower::new(path.nodes, s.pos, 0.25);
            }
            other => panic!("tick {tick}: {other:?} at {:?}", s.pos),
        }
        m.tick(&world);
    }
    panic!("not arrived: at {:?}", m.position());
}

fn assert_arrived(o: &Outcome) {
    let d = (o.end[0] - GOAL[0] as f32 - 0.5).hypot(o.end[2] - GOAL[2] as f32 - 0.5);
    assert!(d < 0.3 && (o.end[1] - 1.0).abs() < 0.1, "ended at {:?}", o.end);
}

#[test]
fn a_leap_executes_exactly_as_predicted() {
    let g = gap();
    for profile in [MovementProfile::Optimal, MovementProfile::Human] {
        let o = drive(&g, leap_path(&g, profile), |_, _| {});
        assert_arrived(&o);
        assert_eq!(o.diverged, 0, "{profile:?}");
        let [r] = o.reports[..] else { panic!("{profile:?}: reports {:?}", o.reports) };
        assert_eq!((r.executed, r.max_error, r.repredicted), (r.predicted, 0.0, 0), "{profile:?}");
    }
}

#[test]
fn a_knock_mid_leap_diverges_and_replans_without_stopping() {
    let g = gap();
    let mut airborne = 0;
    let o = drive(&g, leap_path(&g, MovementProfile::Optimal), |_, m| {
        if !m.on_ground() {
            airborne += 1;
        }
        if airborne == 3 {
            airborne += 1;
            let knock = SetEntityMotion { runtime_entity_id: 1, velocity: Vec3f { x: -0.5, y: 0.0, z: 0.0 }, tick: 0 };
            m.apply(&raw(&knock), &ME).unwrap();
        }
    });
    assert_arrived(&o);
    assert!(o.diverged >= 1 && o.held, "diverged {} times, held {}", o.diverged, o.held);
    assert!(o.reports.is_empty(), "the leap did not finish: {:?}", o.reports);
}
