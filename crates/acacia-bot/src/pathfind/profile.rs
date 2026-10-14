//! How the bot moves: frame-perfect (`Optimal`) or like a person (`Human`). One profile drives both the
//! planner's rollouts and the follower, so plans only contain moves the follower can repeat.

use crate::cadence::splitmix;
use crate::human;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MovementProfile {
    #[default]
    Optimal,
    Human,
}

/// One way execution may differ from the plan: the jump `jump` ticks late (negative: early), the aim off by
/// `yaw` degrees, the start `start` blocks back along the move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Perturbation {
    pub jump: i8,
    pub yaw: f32,
    pub start: f32,
}

impl Perturbation {
    pub const NOMINAL: Perturbation = Perturbation { jump: 0, yaw: 0.0, start: 0.0 };
}

const HUMAN_PERTURBATIONS: [Perturbation; 7] = [
    Perturbation::NOMINAL,
    Perturbation { jump: 1, ..Perturbation::NOMINAL },
    Perturbation { jump: -1, ..Perturbation::NOMINAL },
    Perturbation { yaw: human::AIM_JITTER_DEG, ..Perturbation::NOMINAL },
    Perturbation { yaw: -human::AIM_JITTER_DEG, ..Perturbation::NOMINAL },
    Perturbation { start: human::START_SLOP, ..Perturbation::NOMINAL },
    Perturbation { start: -human::START_SLOP, ..Perturbation::NOMINAL },
];

impl MovementProfile {
    /// Largest difference (blocks) between the live body and the predicted one the follower accepts.
    pub fn tolerance(self) -> f32 {
        match self {
            MovementProfile::Optimal => 1e-4,
            MovementProfile::Human => 0.03,
        }
    }

    /// The executions a macro move must survive to be planned.
    pub fn perturbations(self) -> &'static [Perturbation] {
        match self {
            MovementProfile::Optimal => &[Perturbation::NOMINAL],
            MovementProfile::Human => &HUMAN_PERTURBATIONS,
        }
    }
}

/// Turns the view towards a wanted yaw: `Optimal` snaps, `Human` eases at a capped rate with jitter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aim {
    pub profile: MovementProfile,
    /// Constant yaw error (degrees), for perturbed rollouts.
    pub bias: f32,
    rng: u64,
}

impl Aim {
    pub fn new(profile: MovementProfile, seed: u64) -> Self {
        Self { profile, bias: 0.0, rng: seed }
    }

    /// The yaw to send this tick when facing `from` and wanting `to` (degrees); a `Human` aim advances its
    /// jitter stream.
    pub fn turn(&mut self, from: f32, to: f32) -> f32 {
        let to = to + self.bias;
        if self.profile == MovementProfile::Optimal {
            return to;
        }
        let diff = (to - from + 180.0).rem_euclid(360.0) - 180.0;
        let step = (diff * human::AIM_EASE).clamp(-human::AIM_MAX_TURN_DEG, human::AIM_MAX_TURN_DEG);
        let unit = (splitmix(&mut self.rng) >> 40) as f32 / (1u64 << 24) as f32;
        from + step + (unit * 2.0 - 1.0) * human::AIM_JITTER_DEG
    }
}
