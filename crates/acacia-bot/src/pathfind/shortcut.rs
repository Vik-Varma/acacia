//! Any-angle shortcuts on flat ground (azalea's simulation executor): when the follower reaches a node, it
//! simulates sprinting (and sprint-jumping) straight at nodes further on and runs the fastest clean one
//! instead of the 8-direction staircase between them.

use acacia_physics::{BlockPos, WorldView};

use super::PathNode;
use super::control::{Controller, LAND_EPS, Phase, centre, standing_on, yaw_to};
use super::costs::{COST_HEURISTIC, HOP_SPEED, MACRO_STARTUP};
use super::leap_moves::RUNNING;
use super::moves::MoveKind;
use super::profile::Aim;
use super::rollout::{Body, Spend};
use super::retry::live_hazard;
use super::solve::Solver;
use crate::movement::{Controls, Rollout};

/// Nodes ahead tried, farthest first.
const AHEAD: [usize; 7] = [20, 15, 10, 5, 4, 3, 2];
/// (jump at all, stop jumping this close to the target).
const STYLES: [(bool, f32); 3] = [(false, 0.0), (true, 1.0), (true, 3.0)];
/// Falling this far below the path means the line left the floor.
const DROP: f32 = 0.6;

/// Sprints straight at `target`, jumping whenever on the ground until within `stop_jump` blocks of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShortcutCtl {
    pub target: BlockPos,
    pub feet: f32,
    pub jump: bool,
    pub stop_jump: f32,
    pub aim: Aim,
    max: u16,
    t: u16,
}

impl Controller for ShortcutCtl {
    fn step(&mut self, b: &Body, _: &mut dyn FnMut(&Controls) -> Body, c: &mut Controls) -> Phase {
        if standing_on(b, self.target, self.feet) {
            return Phase::Done;
        }
        if self.t >= self.max || b.pos[1] < self.feet - DROP || (b.on_ground && (b.pos[1] - self.feet).abs() > LAND_EPS) {
            return Phase::Failed;
        }
        self.t += 1;
        let to = centre(self.target);
        (c.strafe, c.sneak, c.pitch, c.forward, c.sprint) = (0.0, false, 0.0, 1.0, true);
        c.yaw = self.aim.turn(b.yaw, yaw_to(b, to));
        c.jump = self.jump && b.on_ground && (b.pos[0] - to[0]).hypot(b.pos[2] - to[1]) > self.stop_jump;
        Phase::Going
    }
}

/// Flat ground at the level of `level`: no work, water or macro leap.
fn flat(n: &PathNode, level: f32) -> bool {
    matches!(n.kind, MoveKind::Walk | MoveKind::Run | MoveKind::Hop) && n.work.is_empty() && !n.wet && (n.feet - level).abs() < LAND_EPS
}

/// The shortcut from the live state to a node after `path[index]`: its controller, predicted bodies and the
/// index it lands on. Only taken when faster than following the nodes between; looked for within `spend`.
pub(super) fn find(path: &[PathNode], index: usize, live: &Rollout, w: &dyn WorldView, aim: Aim, spend: &mut Spend) -> Option<(ShortcutCtl, Vec<Body>, usize)> {
    let level = live.st.pos[1];
    if !live.st.on_ground || !path.get(index).is_some_and(|n| flat(n, level)) {
        return None;
    }
    let reach = path[index..].iter().take_while(|n| flat(n, level)).count();
    let avoid = |p: BlockPos| live_hazard(w, p);
    let held = Controls { yaw: live.st.yaw, ..Controls::default() };
    let mut solver = Solver::new(w, &avoid, &|_| false, aim.profile, false, held);
    solver.budget = spend.left();
    // From standing, following the nodes pays the run-up too.
    let startup = if live.st.vel[0].hypot(live.st.vel[2]) < RUNNING { MACRO_STARTUP } else { 0.0 };
    let found = AHEAD.into_iter().filter(|&k| k < reach).find_map(|k| {
        let j = index + k;
        let along = polyline(live, &path[index..=j]);
        let per_block = if path[index..=j].iter().any(|n| n.kind == MoveKind::Hop) { 1.0 / HOP_SPEED } else { COST_HEURISTIC };
        // By distance, not node count: a hop node spans four blocks.
        let max = (4.0 * along) as u16 + 20;
        let best = STYLES
            .iter()
            .filter_map(|&(jump, stop_jump)| {
                let ctl = ShortcutCtl { target: path[j].pos, feet: path[j].feet, jump, stop_jump, aim, max, t: 0 };
                if !solver.affords(u64::from(max)) {
                    return None;
                }
                let (bodies, _, clean) = solver.run(live, &mut { ctl }, false);
                clean.then_some((ctl, bodies))
            })
            .min_by_key(|(_, b)| b.len());
        best.filter(|(_, b)| ((b.len() - 1) as f32) < along * per_block + startup).map(|(ctl, bodies)| (ctl, bodies, j))
    });
    spend.charge(&solver);
    found
}

/// Blocks from the live position through the nodes' centres.
fn polyline(live: &Rollout, nodes: &[PathNode]) -> f32 {
    let mut at = [live.st.pos[0], live.st.pos[2]];
    let mut len = 0.0;
    for n in nodes {
        let c = centre(n.pos);
        len += (c[0] - at[0]).hypot(c[1] - at[1]);
        at = c;
    }
    len
}
