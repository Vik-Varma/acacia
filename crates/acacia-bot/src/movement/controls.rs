use acacia_client::proto::packets::{PlayerAuthInput, PlayerAuthInputBlockActionItem};
use acacia_client::proto::types::InputData;
use acacia_physics::Vec3;

/// The server's state of a vehicle the bot drives (`CorrectPlayerMovePrediction` type Vehicle).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VehicleCorrection {
    pub feet: Vec3,
    pub delta: Vec3,
    pub on_ground: bool,
    pub pitch_yaw: [f32; 2],
    /// The input tick the server's state is from.
    pub tick: u64,
    /// A boat's turn per tick, in degrees.
    pub yaw_velocity: Option<f32>,
}

/// What the bot is trying to do this tick; persists until changed.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Controls {
    /// +1 = walk forward, -1 = back. Sent as a keyboard key, so any nonzero value counts as ±1:
    /// BDS moves keyboard players at full speed whatever the move vector says.
    pub forward: f32,
    /// +1 = strafe left, -1 = right; ±1 like `forward`.
    pub strafe: f32,
    pub jump: bool,
    pub sneak: bool,
    pub sprint: bool,
    /// Keep an elytra open. Set in the air to start a glide; the simulation clears it when the glide ends
    /// (landing, water) or cannot start.
    pub glide: bool,
    /// Fly when the abilities allow it: a change double-taps jump for 4 ticks, as vanilla toggles a flight.
    /// While flying, jump rises and sneak descends. Follows the flight when it changes otherwise.
    pub fly: bool,
    /// Degrees; 0 faces +Z, wrapped to -180..=180 when sent.
    pub yaw: f32,
    /// Degrees; -90 looks up, clamped to -90..=90 when sent.
    pub pitch: f32,
}

impl Controls {
    pub fn stop(&mut self) {
        (self.forward, self.strafe, self.jump, self.sprint) = (0.0, 0.0, false, false);
    }

    /// Keeps the world direction `course` walks in while facing this yaw: the nearest of the eight key
    /// combinations (at most 22.5° off; the follower corrects it next tick), without sprint (it needs forward).
    pub fn keep_course(&mut self, course: &Controls) {
        if course.forward == 0.0 && course.strafe == 0.0 {
            return;
        }
        let heading = course.yaw + (-course.strafe).atan2(course.forward).to_degrees();
        let rel = ((heading - self.yaw + 540.0).rem_euclid(360.0) - 180.0) / 45.0;
        let r = (rel.round() * 45.0).to_radians();
        (self.forward, self.strafe, self.sprint) = (r.cos().round(), (-r.sin()).round(), false);
    }

    /// Turns to face `target` from `eye` (both world positions).
    pub fn look_at(&mut self, eye: Vec3, target: Vec3) {
        let [dx, dy, dz] = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
        self.yaw = (-dx).atan2(dz).to_degrees();
        self.pitch = (-dy).atan2((dx * dx + dz * dz).sqrt()).to_degrees();
    }
}

/// Adds queued block actions (server-authoritative breaking) to a tick's input.
pub(crate) fn attach_actions(input: &mut PlayerAuthInput, actions: Vec<PlayerAuthInputBlockActionItem>) {
    if !actions.is_empty() {
        input.input_data.push(InputData::BlockAction);
        input.block_action = Some(actions);
    }
}
