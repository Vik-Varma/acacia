//! What a controller sees of the player each tick, in rollouts and live alike.

use acacia_physics::{PlayerState, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    /// Feet.
    pub pos: Vec3,
    pub vel: Vec3,
    /// Degrees, as last sent.
    pub yaw: f32,
    pub on_ground: bool,
    pub h_collision: bool,
}

impl Body {
    pub fn of(st: &PlayerState) -> Self {
        Self { pos: st.pos, vel: st.vel, yaw: st.yaw, on_ground: st.on_ground, h_collision: st.collide_x || st.collide_z }
    }

    /// Largest per-axis position difference.
    pub fn error(&self, other: &Body) -> f32 {
        (0..3).map(|i| (self.pos[i] - other.pos[i]).abs()).fold(0.0, f32::max)
    }
}
