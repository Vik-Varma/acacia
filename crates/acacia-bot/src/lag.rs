//! Injected inbound lag, for tests and the lab ([`crate::BotConfig::lag`]): every server packet is held back
//! before the bot sees it. RakNet resends lost datagrams in order, so a loss shows up as a delay spike that
//! holds back everything behind it, not as a missing packet.

use std::collections::VecDeque;
use std::time::Duration;

use acacia_client::proto::RawPacket;
use tokio::time::Instant;

use crate::cadence::splitmix;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lag {
    /// Added to every packet.
    pub delay: Duration,
    /// Up to this much more, uniformly.
    pub jitter: Duration,
    /// Chance a packet is "lost" and resent: it, and every packet behind it, waits `spike` more.
    pub loss: f32,
    pub spike: Duration,
    pub seed: u64,
}

impl Lag {
    /// A plain one-way delay.
    pub fn fixed(delay: Duration) -> Self {
        Self { delay, jitter: Duration::ZERO, loss: 0.0, spike: Duration::ZERO, seed: 1 }
    }
}

pub(crate) struct LagLine {
    lag: Lag,
    queue: VecDeque<(Instant, RawPacket)>,
    rng: u64,
}

impl LagLine {
    pub(crate) fn new(lag: Lag) -> Self {
        Self { lag, queue: VecDeque::new(), rng: lag.seed }
    }

    fn unit(&mut self) -> f32 {
        (splitmix(&mut self.rng) >> 40) as f32 / (1u64 << 24) as f32
    }

    pub(crate) fn push(&mut self, now: Instant, packet: RawPacket) {
        let mut at = now + self.lag.delay + self.lag.jitter.mul_f32(self.unit());
        if self.unit() < self.lag.loss {
            at += self.lag.spike;
        }
        // In order: nothing overtakes a held-back packet.
        if let Some((last, _)) = self.queue.back() {
            at = at.max(*last);
        }
        self.queue.push_back((at, packet));
    }

    pub(crate) fn next_at(&self) -> Option<Instant> {
        self.queue.front().map(|(at, _)| *at)
    }

    pub(crate) fn pop_due(&mut self, now: Instant) -> Option<RawPacket> {
        if self.next_at()? > now {
            return None;
        }
        self.queue.pop_front().map(|(_, p)| p)
    }
}
