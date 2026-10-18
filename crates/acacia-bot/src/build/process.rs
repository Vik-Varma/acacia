//! [`Builder`] as a [`Process`]: survey, pick the next step (`plan`), drive or act, learn from the outcome.

use super::builder::Builder;
use super::plan::{self, Next};
use super::status::Phase;
use crate::pathfind::process::{Act, Ctx, Event, Process, Want};
use crate::pathfind::GotoOpts;
use crate::ActionError;

/// Ticks of waiting before stations are asked again (they may have been refilled).
const ASK_AGAIN: u64 = 600;

impl Process for Builder {
    fn name(&self) -> &str {
        "builder"
    }

    fn want(&mut self, cx: &Ctx) -> Want {
        self.tick = cx.tick;
        self.check_restock(cx.look);
        let needs = self.survey(cx.look);
        self.sync(cx, &needs);
        self.scaffold.retain(|p| cx.look.solid(*p) != Some(false));
        // In the air: keep driving (a jump or pillar in progress), or wait to land before acting.
        let mut next = match &self.commit {
            Some((task, goal)) if !cx.on_ground => Next::Go(Phase::Building, *task, goal.clone()),
            _ => Next::Hold(Phase::Building, Vec::new()),
        };
        // A task wanting the goal just reached could not act from there after all: let it rest and ask again.
        let mut planned = false;
        for _ in 0..8 {
            if !cx.on_ground {
                break;
            }
            (next, planned) = (plan::next(self, cx, &needs), true);
            match &next {
                Next::Go(_, task, goal) if self.arrived.as_ref() == Some(goal) => {
                    tracing::debug!(?task, ?goal, "build stand reached but no click from there");
                    self.rest(*task);
                    (next, planned) = (Next::Hold(Phase::Building, Vec::new()), false);
                }
                _ => break,
            }
        }
        if planned && matches!(&next, Next::Hold(p, _) if *p != Phase::Restocking) {
            self.exhausted();
        }
        let (phase, missing, want) = match next {
            Next::Act(phase, act) => {
                self.arrived = None;
                (phase, Vec::new(), Want::Act(act))
            }
            Next::Go(phase, task, goal) => {
                self.commit = Some((task, goal.clone()));
                (phase, Vec::new(), Want::Go(goal))
            }
            Next::Hold(phase, missing) => (phase, missing, Want::Hold),
            Next::Done(result) => {
                self.leave();
                (Phase::Done, Vec::new(), Want::Done(result))
            }
        };
        if phase == Phase::Waiting {
            let since = *self.waiting_since.get_or_insert(self.tick);
            if self.tick - since > ASK_AGAIN {
                self.stations.iter_mut().for_each(|s| s.lacks.clear());
                self.waiting_since = None;
            }
        } else {
            self.waiting_since = None;
        }
        self.publish(&needs, cx.look, phase, missing);
        want
    }

    fn opts(&self) -> GotoOpts {
        let mut opts = GotoOpts { scaffold: self.job.scaffold.clone(), ..GotoOpts::default() };
        opts.search.allow_bridge = true;
        opts
    }

    fn arrived(&mut self) -> bool {
        self.arrived = self.commit.take().map(|(_, goal)| goal);
        false
    }

    fn failed(&mut self, e: &ActionError) -> bool {
        if let Some((task, _)) = self.commit.take() {
            self.fail(task, e);
        }
        false
    }

    fn acted(&mut self, act: &Act, result: &Result<(), ActionError>) {
        match (act, result) {
            (Act::Place { against, face, .. }, Ok(())) => {
                self.tries.entry(face.adjacent(*against)).or_default().places += 1;
                self.bump(|s| s.placed += 1);
            }
            (Act::Place { against, face, .. }, Err(e)) => self.fail(face.adjacent(*against), e),
            (Act::Break { .. }, Ok(())) => self.bump(|s| s.broken += 1),
            (Act::Break { pos }, Err(e)) => self.fail(*pos, e),
            (Act::Fill { pos, .. }, Ok(())) => {
                if self.index.contains_key(pos) || !self.seals.contains(pos) {
                    self.add_scaffold(*pos);
                }
            }
            (Act::Fill { pos, .. }, Err(e)) => {
                self.seals.remove(pos);
                self.fail(*pos, e);
            }
            (Act::Restock { station, .. }, r) => {
                self.close_station(*station);
                if let Err(e) = r {
                    tracing::debug!(%e, "restock failed");
                }
                self.bump(|s| s.restocks += 1);
            }
        }
    }

    fn observe(&mut self, event: &Event) {
        if let Event::Placed { pos } = event {
            self.add_scaffold(*pos);
        }
    }

    fn lost_control(&mut self) {
        self.commit = None;
    }
}
