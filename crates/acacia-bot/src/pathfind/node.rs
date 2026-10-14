//! Search node state beyond the block position: the player's motion at a node, quantized so nodes
//! reached at nearly the same speed share a cache entry.

use std::collections::HashMap;
use std::hash::BuildHasherDefault;

use acacia_physics::constants::{JUMP_DELAY_TICKS, SNEAKING_PLAYER_HEIGHT};
use acacia_physics::{BlockPos, PlayerState, Vec3};

use super::terrain::PosHasher;

/// How the player moves at a node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Ground,
}

/// The motion a node is entered with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Entry {
    /// Standing (classic and work moves end here).
    #[default]
    Rest,
    /// Running along direction index 0..8 ([`DIRS`]) in a horizontal speed bin ([`speed_bin`]), so a
    /// faster later arrival is not pruned by a slower earlier one.
    Run(u8, u8),
    /// In the air; the search start only.
    Air,
}

/// The A* key: a block position with how the player is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeKey {
    pub pos: BlockPos,
    pub mode: Mode,
    pub entry: Entry,
}

impl NodeKey {
    pub fn rest(pos: BlockPos) -> Self {
        Self { pos, mode: Mode::Ground, entry: Entry::Rest }
    }
}

/// Upper edges (blocks/tick) of the `Entry::Run` speed bins below the top one: walking, sprinting on
/// the ground, sprint-jumping. Placeholders until hop speeds are measured (plan step 6).
const SPEED_BINS: [f32; 3] = [0.15, 0.25, 0.32];

/// The `Entry::Run` bin (0..=3) of a horizontal speed.
pub fn speed_bin(h_speed: f32) -> u8 {
    SPEED_BINS.iter().filter(|&&edge| h_speed >= edge).count() as u8
}

/// Horizontal directions, indexed by `Entry::Run`.
pub(crate) const DIRS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The `Entry::Run` of moving at `vel`: the nearest of [`DIRS`] and the speed's bin.
pub(crate) fn run_entry(vel: Vec3) -> Entry {
    let speed = vel[0].hypot(vel[2]);
    // sin 22.5°: inside the 45° sector around an axis, the other axis reads 0.
    let axis = |v: f32| if v > 0.383 * speed { 1 } else if v < -0.383 * speed { -1 } else { 0 };
    let d = (axis(vel[0]), axis(vel[2]));
    let dir = DIRS.iter().position(|&x| x == d).unwrap_or(0);
    Entry::Run(dir as u8, speed_bin(speed))
}

pub(crate) type KeyMap<V> = HashMap<NodeKey, V, BuildHasherDefault<PosHasher>>;

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
        let bit = |on: bool, flag: u8| if on { flag } else { 0 };
        let flags = bit(st.on_ground, ON_GROUND)
            | bit(st.sprinting, SPRINTING)
            | bit(st.sneaking, SNEAKING)
            | bit(st.pressing_jump, PRESSING_JUMP)
            | (delay << DELAY_SHIFT);
        Self { off, vel: st.vel.map(|v| q(v, VEL_Q)), yaw_q: (st.yaw.rem_euclid(360.0) * YAW_Q) as u16, flags }
    }

    /// Standing still at the centre of block `at` with feet at `feet`.
    pub fn rest(feet: f32, at: BlockPos) -> Self {
        let off = [OFF_Q as i16 / 2, q(feet - at[1] as f32, OFF_Q), OFF_Q as i16 / 2];
        Self { off, flags: ON_GROUND, ..Self::default() }
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
