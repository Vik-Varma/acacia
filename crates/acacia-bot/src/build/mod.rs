//! Schematic building: a `.mcstructure` placed block-exact by a [`Builder`] process, which re-surveys the world
//! every tick (so a restart resumes from what is there), restocks from stations by remaining need, and cleans
//! up its scaffold. Several builders can share a job through a [`TaskBoard`]. Design: acacia-pathfind/docs/builder.md.

mod adjust;
mod ahead;
mod arrival;
mod bill;
mod board;
mod builder;
mod check;
mod cleanup;
mod cover;
mod en_route;
mod explain;
pub mod findings;
mod job;
pub mod journal;
mod pairs;
mod plan;
mod process;
mod reach;
mod restock;
mod rules;
pub mod sight_stats;
mod status;
mod structure;
mod support;
mod survey;
mod swarm;
#[cfg(test)]
pub(crate) mod tests;
mod transform;
mod wedged;

pub use bill::bill;
pub use board::{LocalBoard, TaskBoard};
pub use builder::{BuildError, Builder};
pub use check::{Mismatch, check, check_text, colors};
pub use job::BuildJob;
pub use rules::supported;
pub use status::{BuildStatus, HoldWhy, Phase, StatusHandle};
pub use structure::{BED_COLOR, Block, SignText, Structure, StructureError};
pub use transform::Transform;
