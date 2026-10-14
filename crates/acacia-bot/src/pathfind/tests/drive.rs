//! Following a path in the headless simulation the way the navigator does: macro moves run live, and a
//! divergence re-plans from the live state without stopping.

use std::time::{Duration, Instant};

use acacia_physics::{BlockPos, Vec3};

use super::grid::Grid;
use super::sim::sense;
use crate::movement::Movement;
use crate::pathfind::{FollowStatus, Follower, Goal, MacroReport, MovementProfile, Path, PathNode, SearchOpts, search_from};

pub fn macro_opts(profile: MovementProfile) -> SearchOpts {
    // Bounded by nodes, not time: debug builds simulate slowly, and a timed-out plan would be a flaky one.
    SearchOpts { max_nodes: 20_000, budget: Duration::from_secs(3600), profile, ..SearchOpts::default() }
}

/// A movement standing at `feet`, facing +x.
pub fn standing(feet: Vec3) -> Movement {
    let mut m = Movement::new();
    m.start(feet, -90.0, 0.0);
    m
}

/// Searches from `m`'s live state, logging the search's size.
pub fn plan(g: &Grid, m: &Movement, goal: &Goal, opts: &SearchOpts) -> Path {
    let t0 = Instant::now();
    let p = search_from(&g.terrain(), m.state().expect("started"), goal, opts);
    let ms = t0.elapsed().as_secs_f32() * 1000.0;
    eprintln!("plan {:?}: {} nodes, {} visited, {} rollouts, {ms:.1} ms, complete {}", opts.profile, p.nodes.len(), p.visited, p.rollouts, p.complete);
    p
}

pub struct Outcome {
    pub reports: Vec<MacroReport>,
    pub diverged: u32,
    /// Forward was still held on every tick that diverged.
    pub held: bool,
    pub ticks: u32,
    pub end: Vec3,
}

/// Follows `path` from `m` towards `goal`; `nudge` may disturb the bot each tick.
pub fn drive(g: &Grid, m: &mut Movement, path: Vec<PathNode>, goal: &Goal, opts: &SearchOpts, mut nudge: impl FnMut(u32, &mut Movement)) -> Outcome {
    let world = g.physics();
    let start = m.position().expect("started");
    let mut follower = Follower::new(path, start, 0.25);
    let mut out = Outcome { reports: Vec::new(), diverged: 0, held: true, ticks: 0, end: start };
    for tick in 0..600 {
        nudge(tick, m);
        let s = sense(g, m);
        let live = m.fork();
        let status = follower.tick(&s, live.as_ref(), &g.terrain(), &mut m.controls);
        out.reports.extend(follower.take_macro_report());
        match status {
            FollowStatus::Moving => {}
            FollowStatus::Arrived => {
                (out.end, out.ticks) = (s.pos, tick);
                return out;
            }
            FollowStatus::Diverged => {
                eprintln!("tick {tick}: diverged at {:?}, controls {:?}", s.pos, m.controls);
                out.diverged += 1;
                out.held &= m.controls.forward != 0.0;
                let path = plan(g, m, goal, opts);
                assert!(path.complete, "no path from {:?}", s.pos);
                follower = Follower::new(path.nodes, s.pos, 0.25);
            }
            other => panic!("tick {tick}: {other:?} at {:?}, next {:?}", s.pos, follower.ahead().first()),
        }
        m.tick(&world);
    }
    panic!("not arrived: at {:?}, next {:?}", m.position(), follower.ahead().first());
}

pub fn assert_arrived(o: &Outcome, goal: BlockPos) {
    let d = (o.end[0] - goal[0] as f32 - 0.5).hypot(o.end[2] - goal[2] as f32 - 0.5);
    assert!(d < 0.3 && (o.end[1] - goal[1] as f32).abs() < 0.1, "ended at {:?}, wanted {goal:?}", o.end);
}

/// Every macro move ran exactly as predicted when it started (the `Optimal` invariant).
pub fn assert_exact(o: &Outcome) {
    for r in &o.reports {
        assert_eq!((r.executed, r.max_error, r.repredicted), (r.predicted, 0.0, 0), "{:?}", o.reports);
    }
}

/// The path's macro-move kinds and landing cells.
pub fn macro_nodes(p: &Path) -> Vec<(crate::pathfind::MoveKind, BlockPos)> {
    p.nodes.iter().filter(|n| n.mac.is_some()).map(|n| (n.kind, n.pos)).collect()
}
