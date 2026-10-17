//! Schematic building: a `.mcstructure` placed block-exact by a [`Builder`] process, which re-surveys the world
//! every tick (so a restart resumes from what is there), restocks from stations by remaining need, and cleans
//! up its scaffold. Design: acacia-pathfind/docs/builder.md.

mod builder;
mod check;
mod job;
mod plan;
mod process;
mod reach;
mod restock;
mod rules;
mod status;
mod structure;
mod survey;
mod transform;
#[cfg(test)]
pub(crate) mod tests;

pub use builder::{BuildError, Builder};
pub use check::{check, Mismatch};
pub use job::BuildJob;
pub use rules::supported;
pub use status::{BuildStatus, Phase, StatusHandle};
pub use structure::{Block, Structure, StructureError};
pub use transform::Transform;
