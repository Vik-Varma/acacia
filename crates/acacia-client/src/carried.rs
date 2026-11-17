//! A connection whose keys something else holds. The carrier owns the transport, negotiates
//! compression, signs and sends the login and exchanges keys; every packet after that crosses
//! here, in whole batches laid out as on the wire before compression and encryption.

use bytes::Bytes;
use tokio::sync::{mpsc, oneshot};

/// The carrier's ends of a connection ([`crate::ClientBuilder::carried`]).
pub struct Carrier {
    /// Told once, when the connection starts, what to log in with.
    pub login: oneshot::Sender<CarriedLogin>,
    /// What the server sent. Dropping the sender ends the connection like [`Carried::Closed`].
    pub inbound: mpsc::UnboundedReceiver<Carried>,
    /// What to send: one message is one batch. The receiver sees it closed when the client leaves.
    pub outbound: mpsc::UnboundedSender<Bytes>,
}

/// What the carrier signs into an offline login.
#[derive(Debug, Clone)]
pub struct CarriedLogin {
    pub protocol: i32,
    pub display_name: String,
    /// The offline identity (a UUID) the name gives.
    pub identity: String,
    /// The login's client data, as JSON.
    pub client_data: String,
}

#[derive(Debug)]
pub enum Carried {
    /// One batch from the server.
    Batch(Bytes),
    /// The carrier's connection ended, with why.
    Closed(String),
}
