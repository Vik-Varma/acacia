//! High-level Bedrock bot on top of `acacia-client`: tracked game state now; physics,
//! interactions and pathfinding follow (see docs/DESIGN.md).
//!
//! Each [`Bot`] owns its state and updates it as packets arrive in [`Bot::next`], so there is no
//! locking and no background work beyond the client's own connection task.

pub mod act_timing;
pub mod books;
mod bot;
pub mod build;
mod cadence;
pub mod cancel;
mod config;
mod elytra;
pub mod events;
mod fishing;
pub mod forms;
pub mod governor;
mod human;
pub mod inbox;
pub mod interact;
pub mod items;
mod lag;
pub mod movement;
pub mod pathfind;
mod prediction_sync;
mod reflex;
mod respawn;
mod riding;
pub mod signs;
mod sleep;
pub mod snapshot;
mod spawn;
pub mod state;
mod subchunks;
pub mod survival;
pub mod swarm;
mod wait;
pub mod workstation;
pub mod world;

pub use wait::ActionError;
pub mod text;
pub mod trace;

pub use acacia_client::{self as client, ConnectError, DisconnectReason, Reason, Violation, proto};
#[cfg(feature = "java")]
pub use acacia_java_client as java_client;
#[cfg(feature = "java")]
pub use bot::Java;
pub use bot::{Bedrock, Bot};
pub use cancel::CancelHandle;
pub use config::BotConfig;
pub use events::{BotEvent, Events};
pub use inbox::{Immediate, Inbox};
pub use lag::Lag;
pub use state::GameState;
