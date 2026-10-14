//! The follower around moves with [`Work`](crate::pathfind::Work): stopping at the segment start for it.

use super::{FollowStatus, Follower, STUCK_TICKS, Sense, hdist};
use crate::movement::Controls;
use crate::pathfind::PathNode;

/// Within this of the segment start (horizontally), the bot does a move's work from where it is.
const WORK_RADIUS: f32 = 0.5;
/// Horizontal speed (blocks per tick) below which the bot counts as standing for work.
const SETTLED: f32 = 0.05;

impl Follower {
    /// The node whose work is due before moving on, if any.
    pub fn pending_work(&self) -> Option<&PathNode> {
        self.has_pending(self.index).then(|| &self.path[self.index])
    }

    pub fn work_done(&mut self) {
        self.worked = true;
        (self.best_dist, self.since_progress) = (f32::INFINITY, 0);
    }

    pub(super) fn has_pending(&self, i: usize) -> bool {
        self.path.get(i).is_some_and(|n| !n.work.is_empty()) && !(i == self.index && self.worked)
    }

    /// The nodes ahead whose footing exists already: up to the first one waiting for work.
    pub fn standing_ahead(&self) -> &[PathNode] {
        let end = (self.index..self.path.len()).find(|&i| self.has_pending(i)).unwrap_or(self.path.len());
        &self.path[self.index..end]
    }

    /// Work is done from the segment start: walk back there if needed and come to a stop.
    pub(super) fn approach_work(&mut self, s: &Sense, c: &mut Controls, speed: f32) -> FollowStatus {
        let d = hdist(s.pos, self.origin);
        c.stop();
        c.sneak = false;
        if d < WORK_RADIUS && s.on_ground {
            return if speed < SETTLED { FollowStatus::Work } else { FollowStatus::Moving };
        }
        self.since_progress += 1;
        if self.since_progress > STUCK_TICKS {
            return FollowStatus::Stuck;
        }
        c.yaw = (s.pos[0] - self.origin[0]).atan2(self.origin[2] - s.pos[2]).to_degrees();
        c.forward = 1.0;
        FollowStatus::Moving
    }
}
