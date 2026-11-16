//! Network-free NetherNet: direct-connect HTTP signaling (both sides), signal text for LAN and the
//! signaling service, the SDP identity assertion, fragment framing and a str0m WebRTC connection
//! that offers (client) or answers (server), and a host holding many answering connections on one
//! socket.
//! Wire specs: docs/research/nethernet-wire.md, docs/research/nethernet-signaling.md.

mod assertion;
mod cert;
mod conn;
mod error;
mod frame;
pub mod host;
pub mod http;
mod identity;
pub mod lan;
#[cfg(test)]
mod loopback_tests;
mod offer_identity;
mod sdp;
mod server_identity;
mod signal;
pub mod signaling;
pub mod turn;

pub use conn::{Connection, Event, LocalCandidate, Transmit};
pub use error::{Error, signaling_error_name};
pub use identity::{AUTH_DOMAIN, Identity};
pub use offer_identity::{OfferIdentity, verify_offer};
pub use server_identity::sign_answer;
pub use signal::{Signal, SignalKind};

/// `GET` path whose 2xx answer means the server speaks NetherNet.
pub const PROBE_PATH: &str = "/v1/join";

/// `POST` path carrying the SDP offer; `network_id` is the client's random u64.
pub fn join_path(network_id: u64) -> String {
    format!("/v1/join/{network_id}")
}
