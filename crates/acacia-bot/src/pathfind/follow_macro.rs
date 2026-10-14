//! Executing a macro move live: the follower hands the move's controller the live body every tick and
//! checks it against the prediction made when the move started.

use acacia_physics::{BlockPos, Vec3, WorldView};

use super::PathNode;
use super::control::{Controller, Macro, Phase};
use super::rollout::{self, Body};
use super::solve::{STRAIGHT, Solver, WIDE, live_hazard};
use crate::movement::{Controls, Rollout};

/// Prediction horizon; controllers fail long before it.
const MAX_TICKS: u32 = 60;

/// How a finished macro move went, for tests and logs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MacroReport {
    /// Ticks the first prediction took.
    pub predicted: usize,
    pub executed: usize,
    /// Largest live-vs-predicted difference seen within tolerance.
    pub max_error: f32,
    /// Times the live body left the tolerance and the move was predicted (or solved) again from it.
    pub repredicted: u32,
    /// The planned controller did not finish from the live start and was solved again.
    pub resolved: bool,
}

pub(super) enum MacroStep {
    /// The current node is no macro move (or has no live state to run one): steer classically.
    Idle,
    Going,
    /// The move finished on the current node this tick.
    Done,
    Diverged,
}

/// The follower's macro side: the move running now, if any.
#[derive(Default)]
pub(super) struct MacroRunner {
    exec: Option<Exec>,
    /// Path index whose macro move has finished.
    done: Option<usize>,
    report: Option<MacroReport>,
}

impl MacroRunner {
    pub fn active(&self) -> bool {
        self.exec.is_some()
    }

    /// The macro move of path index `index` has run.
    pub fn finished(&self, index: usize) -> bool {
        self.done == Some(index)
    }

    pub fn take_report(&mut self) -> Option<MacroReport> {
        self.report.take()
    }

    /// Runs (starting if due) the macro move of `path[index]`, setting `c` for the tick after `live`.
    pub fn tick(&mut self, path: &[PathNode], index: usize, live: Option<&Rollout>, w: &dyn WorldView, c: &mut Controls) -> MacroStep {
        let (Some(live), Some(mac)) = (live, path[index].mac) else { return MacroStep::Idle };
        if self.done == Some(index) {
            return MacroStep::Idle;
        }
        let exec = match &mut self.exec {
            Some(e) => e,
            None => match Exec::start(mac, path[index].mom.velocity(), live, *c, w) {
                Some(e) => self.exec.insert(e),
                None => return MacroStep::Diverged,
            },
        };
        match exec.tick(live, w, c) {
            Phase::Going => MacroStep::Going,
            Phase::Done => {
                (self.report, self.done) = (Some(exec.report), Some(index));
                self.exec = None;
                MacroStep::Done
            }
            Phase::Failed => {
                self.exec = None;
                MacroStep::Diverged
            }
        }
    }
}

struct Exec {
    ctl: Macro,
    predicted: Vec<Body>,
    /// Index into `predicted` of the body expected now.
    t: usize,
    tolerance: f32,
    /// The planned arrival velocity.
    want: Vec3,
    report: MacroReport,
}

/// Arriving this much faster or slower (blocks/tick) than planned starts the next move off its plan.
const ARRIVAL_SLOP: f32 = 0.03;

impl Exec {
    /// Predicts `ctl` from the live state; solves it again if it does not finish from there, or arrives
    /// too far from the planned velocity `want`.
    fn start(ctl: Macro, want: Vec3, live: &Rollout, c: Controls, w: &dyn WorldView) -> Option<Self> {
        let off = |p: &(Macro, Vec<Body>)| {
            let end = p.1[p.1.len() - 1].vel;
            (end[0] - want[0]).hypot(end[2] - want[2])
        };
        let planned = predict(ctl, live, c, w).filter(|p| off(p) <= ARRIVAL_SLOP);
        let ((ctl, predicted), resolved) = match planned {
            Some(p) => (p, false),
            None => (resolve(&ctl, want, live, c, w).or_else(|| predict(ctl, live, c, w))?, true),
        };
        let report = MacroReport { predicted: predicted.len() - 1, executed: 0, max_error: 0.0, repredicted: 0, resolved };
        Some(Self { ctl, predicted, t: 0, tolerance: ctl.profile().tolerance(), want, report })
    }

    /// Steps the controller on the live body, predicting again from it when it left the tolerance.
    /// `Failed` when nothing from the live body finishes the move.
    fn tick(&mut self, live: &Rollout, w: &dyn WorldView, c: &mut Controls) -> Phase {
        let b = Body::of(&live.st);
        let err = self.predicted.get(self.t).map_or(f32::INFINITY, |p| p.error(&b));
        if err > self.tolerance {
            let again = predict(self.ctl, live, *c, w).or_else(|| resolve(&self.ctl, self.want, live, *c, w));
            let Some((ctl, again)) = again else { return Phase::Failed };
            tracing::debug!(t = self.t, err, "macro move off its prediction; predicted again");
            (self.ctl, self.predicted, self.t) = (ctl, again, 0);
            self.report.repredicted += 1;
        } else {
            self.report.max_error = self.report.max_error.max(err);
        }
        let phase = self.ctl.step(&b, &mut |pc: &Controls| rollout::peek(live, pc, w), c);
        if phase == Phase::Going {
            self.t += 1;
            self.report.executed += 1;
        }
        phase
    }
}

/// `ctl` run on from the live state, if it finishes.
fn predict(mut ctl: Macro, live: &Rollout, c: Controls, w: &dyn WorldView) -> Option<(Macro, Vec<Body>)> {
    let start = ctl;
    let trace = rollout::run(&mut live.clone(), &mut ctl, c, MAX_TICKS, w);
    trace.landed.then_some((start, trace.bodies))
}

/// Another way to finish `ctl`'s move from the live state, arriving near velocity `want`: a leap's other
/// timings or coasts.
fn resolve(ctl: &Macro, want: Vec3, live: &Rollout, held: Controls, w: &dyn WorldView) -> Option<(Macro, Vec<Body>)> {
    let avoid = |p: BlockPos| live_hazard(w, p);
    match ctl {
        Macro::Leap(l) => {
            let profile = l.aim.profile;
            let mut s = Solver { w, avoid: &avoid, pad: profile.hazard_pad(), safe: &|_| false, profile, perturb: false, held, rollouts: 0 };
            let found = s.retry(l, live, STRAIGHT, want).or_else(|| (!l.airborne()).then(|| s.retry(l, live, WIDE, want)).flatten());
            found.map(|(l, b)| (Macro::Leap(l), b))
        }
        Macro::Run(_) => None,
    }
}
