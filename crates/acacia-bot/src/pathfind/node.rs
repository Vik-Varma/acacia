//! Search node state beyond the block position: the player's motion at a node, quantized so nodes
//! reached at nearly the same speed share a cache entry.

use acacia_physics::constants::{JUMP_DELAY_TICKS, SNEAKING_PLAYER_HEIGHT};
use acacia_physics::{BlockPos, PlayerState, Vec3};

/// Feet offset steps per block.
const OFF_Q: f32 = 1024.0;
/// Velocity steps per block per tick.
const VEL_Q: f32 = 4096.0;
const YAW_Q: f32 = 65536.0 / 360.0;

const ON_GROUND: u8 = 1;
const SPRINTING: u8 = 1 << 1;
const SNEAKING: u8 = 1 << 2;
const PRESSING_JUMP: u8 = 1 << 3;
/// `jump_delay` lives in the high nibble.
const DELAY_SHIFT: u8 = 4;

/// The motion a node is reached with: feet offset from the node's block corner, velocity, yaw and the
/// flags the next tick reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Momentum {
    pub off: [i16; 3],
    pub vel: [i16; 3],
    pub yaw_q: u16,
    pub flags: u8,
}

fn q(v: f32, scale: f32) -> i16 {
    (v * scale).round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

impl Momentum {
    /// The motion of `st`, with feet measured from block `at`.
    pub fn snap(st: &PlayerState, at: BlockPos) -> Self {
        let off = [0, 1, 2].map(|i| q(st.pos[i] - at[i] as f32, OFF_Q));
        let delay = st.jump_delay.min(JUMP_DELAY_TICKS) as u8;
        let flags = u8::from(st.on_ground) * ON_GROUND
            | u8::from(st.sprinting) * SPRINTING
            | u8::from(st.sneaking) * SNEAKING
            | u8::from(st.pressing_jump) * PRESSING_JUMP
            | delay << DELAY_SHIFT;
        Self { off, vel: st.vel.map(|v| q(v, VEL_Q)), yaw_q: (st.yaw.rem_euclid(360.0) * YAW_Q) as u16, flags }
    }

    pub fn feet(&self, at: BlockPos) -> Vec3 {
        [0, 1, 2].map(|i| at[i] as f32 + self.off[i] as f32 / OFF_Q)
    }

    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    /// A fresh simulation state moving like this at block `at`; attributes, effects, equipment and flight
    /// come from `base` (the live state), everything transient starts from rest.
    pub fn synth(&self, at: BlockPos, base: &PlayerState) -> PlayerState {
        let mut st = PlayerState::new(self.feet(at));
        st.vel = self.vel.map(|v| v as f32 / VEL_Q);
        st.last_vel = st.vel;
        st.yaw = self.yaw_q as f32 / YAW_Q;
        st.on_ground = self.has(ON_GROUND);
        st.sprinting = self.has(SPRINTING);
        st.sneaking = self.has(SNEAKING);
        if st.sneaking {
            st.size[1] = SNEAKING_PLAYER_HEIGHT;
        }
        st.pressing_jump = self.has(PRESSING_JUMP);
        st.jump_delay = u64::from(self.flags >> DELAY_SHIFT);
        st.effects = base.effects;
        st.equipment = base.equipment;
        st.flight = base.flight;
        st.jump_strength = base.jump_strength;
        st.freeze = base.freeze;
        st.set_movement_attribute(base.movement_attribute);
        st
    }
}
