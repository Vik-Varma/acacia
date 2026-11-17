use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use acacia_raknet::{self as raknet, Reliability};
use bytes::Bytes;

pub enum LinkConfig {
    RakNet(raknet::Config),
    /// A message transport (NetherNet) that the driver keeps connected and reliable.
    Message,
    /// A carrier that owns the connection's keys: it has negotiated compression, sent the login
    /// and exchanged keys before the first message, and messages cross it as plain batches.
    Carried,
}

/// What carries game messages: RakNet datagrams, or whole messages handed over by the driver.
pub(super) enum Link {
    RakNet(Box<raknet::Client>),
    Message {
        inbound: VecDeque<raknet::Event>,
        outbound: VecDeque<Bytes>,
    },
}

impl Link {
    pub fn new(cfg: LinkConfig, server: SocketAddr, now: Instant) -> Self {
        match cfg {
            LinkConfig::RakNet(cfg) => {
                Self::RakNet(Box::new(raknet::Client::new(cfg, server, now)))
            }
            LinkConfig::Message | LinkConfig::Carried => Self::Message {
                inbound: VecDeque::from([raknet::Event::Connected { mtu: 0 }]),
                outbound: VecDeque::new(),
            },
        }
    }

    pub fn is_message(&self) -> bool {
        matches!(self, Self::Message { .. })
    }

    pub fn rtt(&self) -> Option<Duration> {
        match self {
            Self::RakNet(r) => r.rtt(),
            Self::Message { .. } => None,
        }
    }

    pub fn is_connected(&self) -> bool {
        match self {
            Self::RakNet(r) => r.is_connected(),
            Self::Message { .. } => true,
        }
    }

    pub fn send(&mut self, batch: Bytes) {
        match self {
            Self::RakNet(r) => {
                r.send(batch, Reliability::ReliableOrdered);
            }
            Self::Message { outbound, .. } => outbound.push_back(batch),
        }
    }

    pub fn handle_datagram(&mut self, now: Instant, data: Bytes) {
        if let Self::RakNet(r) = self {
            r.handle_datagram(now, data);
        }
    }

    pub fn handle_message(&mut self, msg: Bytes) {
        if let Self::Message { inbound, .. } = self {
            inbound.push_back(raknet::Event::Message(msg));
        }
    }

    pub fn handle_timeout(&mut self, now: Instant) {
        if let Self::RakNet(r) = self {
            r.handle_timeout(now);
        }
    }

    pub fn poll_timeout(&self) -> Option<Instant> {
        match self {
            Self::RakNet(r) => r.poll_timeout(),
            Self::Message { .. } => None,
        }
    }

    /// RakNet datagrams, or whole game messages on a message link.
    pub fn poll_transmit(&mut self, now: Instant) -> Option<Bytes> {
        match self {
            Self::RakNet(r) => r.poll_transmit(now),
            Self::Message { outbound, .. } => outbound.pop_front(),
        }
    }

    pub fn poll_event(&mut self) -> Option<raknet::Event> {
        match self {
            Self::RakNet(r) => r.poll_event(),
            Self::Message { inbound, .. } => inbound.pop_front(),
        }
    }

    /// Closes the link and drops whatever it had not yet delivered.
    pub fn close(&mut self, now: Instant) {
        match self {
            Self::RakNet(r) => {
                r.close(now);
                while r.poll_event().is_some() {}
            }
            Self::Message { inbound, .. } => inbound.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use acacia_proto::packets::{Disconnect, RequestNetworkSettings};
    use acacia_proto::{Packet, RawPacket};
    use p384::ecdsa::SigningKey;

    use super::LinkConfig;
    use crate::batch::BatchCodec;
    use crate::pack_store::MemoryPackStore;
    use crate::session::{Session, SessionConfig};

    fn session(link: LinkConfig, now: Instant) -> Session {
        let cfg = SessionConfig {
            link,
            key: SigningKey::from_slice(&[7; 48]).unwrap(),
            login_request: Vec::new(),
            chunk_radius: 2,
            auto_respawn: false,
            initialize_on_spawn: true,
            blob_store: None,
            pack_store: Arc::new(MemoryPackStore::default()),
            strict: false,
            max_batch_bytes: crate::batch::DEFAULT_MAX_DECOMPRESSED,
        };
        Session::new(cfg, "127.0.0.1:19132".parse().unwrap(), now)
    }

    /// The IDs of the packets the session has queued to send.
    fn sent(s: &mut Session, now: Instant) -> Vec<u32> {
        let mut packets = Vec::new();
        while let Some(msg) = s.poll_transmit(now) {
            BatchCodec::without_header()
                .decode(&msg, &mut packets)
                .unwrap();
        }
        packets
            .into_iter()
            .map(|p| RawPacket::parse(p).unwrap().id)
            .collect()
    }

    #[test]
    fn a_message_link_opens_the_login_and_says_goodbye_itself() {
        let now = Instant::now();
        let mut s = session(LinkConfig::Message, now);
        assert_eq!(sent(&mut s, now), [RequestNetworkSettings::ID]);
        s.close(now);
        assert_eq!(sent(&mut s, now), [Disconnect::ID]);
    }

    #[test]
    fn a_carried_link_leaves_the_login_and_the_goodbye_to_its_carrier() {
        let now = Instant::now();
        let mut s = session(LinkConfig::Carried, now);
        assert_eq!(sent(&mut s, now), [] as [u32; 0]);
        s.close(now);
        assert_eq!(sent(&mut s, now), [] as [u32; 0]);
    }
}
