//! Executing a macro move live: the follower hands the move's controller the live body every tick and
//! checks it against the prediction made when the move started.

use acacia_physics::{BlockPos, LiquidKind, WorldView};

use super::PathNode;
use super::control::{Controller, Macro, Phase};
use super::rollout::{self, Body};
use super::solve::{STRAIGHT, Solver};
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
            None => match Exec::start(mac, live, *c, w) {
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
    report: MacroReport,
}

impl Exec {
    /// Predicts `ctl` from the live state, solving it again if it does not finish from there.
    fn start(ctl: Macro, live: &Rollout, c: Controls, w: &dyn WorldView) -> Option<Self> {
        let ((ctl, predicted), resolved) = match predict(ctl, live, c, w) {
            Some(p) => (p, false),
            None => (resolve(&ctl, live, c, w)?, true),
        };
        let report = MacroReport { predicted: predicted.len() - 1, executed: 0, max_error: 0.0, repredicted: 0, resolved };
        Some(Self { ctl, predicted, t: 0, tolerance: ctl.profile().tolerance(), report })
    }

    /// Steps the controller on the live body, predicting again from it when it left the tolerance.
    /// `Failed` when nothing from the live body finishes the move.
    fn tick(&mut self, live: &Rollout, w: &dyn WorldView, c: &mut Controls) -> Phase {
        let b = Body::of(&live.st);
        let err = self.predicted.get(self.t).map_or(f32::INFINITY, |p| p.error(&b));
        if err > self.tolerance {
            let Some((ctl, again)) = predict(self.ctl, live, *c, w).or_else(|| resolve(&self.ctl, live, *c, w)) else { return Phase::Failed };
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

/// Another way to finish `ctl`'s move from the live state: a leap's other timings or coasts.
fn resolve(ctl: &Macro, live: &Rollout, held: Controls, w: &dyn WorldView) -> Option<(Macro, Vec<Body>)> {
    let avoid = |p: BlockPos| {
        let b = w.block(p);
        b.cobweb || b.liquid.is_some_and(|l| l.kind == LiquidKind::Lava)
    };
    match ctl {
        Macro::Leap(l) => {
            let profile = l.aim.profile;
            let mut s = Solver { w, avoid: &avoid, pad: profile.hazard_pad(), profile, perturb: false, held, rollouts: 0 };
            s.retry(l, live, STRAIGHT).map(|(l, b)| (Macro::Leap(l), b))
        }
    }
}
