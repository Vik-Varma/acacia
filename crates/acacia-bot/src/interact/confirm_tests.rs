//! Confirmation against a toy server with injected lag: actions reach it after `up` ticks (or never: lost),
//! its block changes reach the bot after `down` ticks (in order, with spikes for resent datagrams).

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use acacia_world::BlockRegistry;

use super::{unpredict, Confirm, Expect, Verdict};
use crate::cadence::splitmix;
use crate::governor::Governor;
use crate::state::ItemStack;

struct Net {
    up: u64,
    down: u64,
    /// Chance an action never reaches the server.
    lost: f32,
    /// Chance an echo is held back `spike` more ticks (and everything behind it).
    late: f32,
    spike: u64,
    /// The server refuses every action.
    refuse: bool,
}

struct World {
    registry: std::sync::Arc<BlockRegistry>,
    server: HashMap<i32, u32>,
    client: HashMap<i32, u32>,
    to_server: VecDeque<(u64, i32, u32)>,
    to_client: VecDeque<(u64, i32, u32)>,
    rng: u64,
    stone: u32,
    air: u32,
}

impl World {
    fn new() -> Self {
        let registry = BlockRegistry::vanilla_arc();
        let stone = registry.states_of("minecraft:stone").next().expect("stone").0;
        let air = registry.states_of("minecraft:air").next().expect("air").0;
        let row: HashMap<i32, u32> = (0..20).map(|x| (x, if x % 2 == 0 { stone } else { air })).collect();
        Self { registry, server: row.clone(), client: row, to_server: VecDeque::new(), to_client: VecDeque::new(), rng: 7, stone, air }
    }

    fn chance(&mut self, p: f32) -> bool {
        ((splitmix(&mut self.rng) >> 40) as f32 / (1u64 << 24) as f32) < p
    }

    fn send(&mut self, net: &Net, now: u64, x: i32, want: u32) {
        if !self.chance(net.lost) {
            self.to_server.push_back((now + net.up, x, want));
        }
    }

    /// One tick: the server applies arrived actions (an unchanged cell is echoed too: a refusal), echoes arrive.
    fn tick(&mut self, net: &Net, now: u64) -> Vec<i32> {
        while self.to_server.front().is_some_and(|(at, ..)| *at <= now) {
            let (_, x, want) = self.to_server.pop_front().expect("due");
            if !net.refuse {
                self.server.insert(x, want);
            }
            let mut at = now + net.down + if self.chance(net.late) { net.spike } else { 0 };
            at = at.max(self.to_client.back().map_or(0, |(t, ..)| *t));
            self.to_client.push_back((at, x, self.server[&x]));
        }
        let mut echoed = Vec::new();
        while self.to_client.front().is_some_and(|(at, ..)| *at <= now) {
            let (_, x, id) = self.to_client.pop_front().expect("due");
            self.client.insert(x, id);
            echoed.push(x);
        }
        echoed
    }

    fn cell(&self, x: i32) -> Option<(u32, &acacia_world::BlockState)> {
        let id = self.client[&x];
        self.registry.get(id).map(|s| (id, s))
    }
}

struct Outcome {
    verdicts: Vec<Verdict>,
    max_tries: u32,
    ticks: u64,
}

/// Flips every cell of the row (breaks the stone, places stone in the gaps) one action at a time, as
/// `break_block` / `place_block` do.
fn flip_row(w: &mut World, net: &Net, g: &mut Governor) -> Outcome {
    let mut now = 0;
    let mut out = Outcome { verdicts: Vec::new(), max_tries: 0, ticks: 0 };
    for x in 0..20 {
        let (expect, want) = if w.client[&x] == w.air { (Expect::Solid, w.stone) } else { (Expect::Gone, w.air) };
        // Placements are predicted, as `place_block` does: the bot's world shows the block at once.
        let place = expect == Expect::Solid;
        let mut c = Confirm::new(expect, Some(w.client[&x]), now, g.timeout_ticks(), 3);
        if place {
            c = c.predicted();
        }
        let old = w.client[&x];
        let send = |w: &mut World, now| {
            w.send(net, now, x, want);
            if place {
                w.client.insert(x, want);
            }
        };
        send(w, now);
        let sent = now;
        let verdict = loop {
            now += 1;
            g.tick(None);
            let echoed = w.tick(net, now).contains(&x);
            let v = c.check(now, w.cell(x), echoed);
            if place && !matches!(v, Verdict::Wait | Verdict::Done) && c.unechoed() {
                w.client.insert(x, old);
            }
            match v {
                Verdict::Wait => {}
                Verdict::Retry => {
                    c.retried(now, g.timeout_ticks());
                    send(w, now);
                }
                v => break v,
            }
        };
        if verdict == Verdict::Done && c.tries() == 1 {
            g.sample(Duration::from_millis(50 * (now - sent)));
        }
        out.max_tries = out.max_tries.max(c.tries());
        out.verdicts.push(verdict);
    }
    // Let late echoes land before comparing worlds.
    for _ in 0..200 {
        now += 1;
        w.tick(net, now);
    }
    out.ticks = now;
    out
}

fn ghosts(w: &World) -> Vec<i32> {
    (0..20).filter(|x| w.client[x] != w.server[x]).collect()
}

#[test]
fn lagging_echoes_confirm_without_ghosts() {
    let net = Net { up: 4, down: 4, lost: 0.0, late: 0.0, spike: 0, refuse: false };
    let (mut w, mut g) = (World::new(), Governor::default());
    let r = flip_row(&mut w, &net, &mut g);
    assert!(r.verdicts.iter().all(|v| *v == Verdict::Done), "{:?}", r.verdicts);
    assert_eq!(r.max_tries, 1, "an 8-tick echo fits the timeout: no retries");
    assert!(ghosts(&w).is_empty(), "client and server differ at {:?}", ghosts(&w));
    let rtt = g.rtt().expect("measured").as_millis();
    assert!((400..=450).contains(&rtt), "echo round-trip {rtt} ms");
}

#[test]
fn lost_actions_and_late_echoes_retry_without_ghosts_or_loops() {
    let net = Net { up: 3, down: 3, lost: 0.15, late: 0.1, spike: 16, refuse: false };
    let (mut w, mut g) = (World::new(), Governor::default());
    let r = flip_row(&mut w, &net, &mut g);
    assert!(r.verdicts.iter().all(|v| *v == Verdict::Done), "{:?}", r.verdicts);
    assert!(r.max_tries > 1, "some action was lost and sent again");
    assert!(r.max_tries <= 3, "no more than 3 tries: {}", r.max_tries);
    assert!(ghosts(&w).is_empty(), "client and server differ at {:?}", ghosts(&w));
    assert!(r.ticks < 20 * 60 + 200, "{} ticks", r.ticks);
}

#[test]
fn refused_actions_fail_after_three_tries_and_leave_the_world_as_the_server_has_it() {
    let net = Net { up: 2, down: 2, lost: 0.0, late: 0.0, spike: 0, refuse: true };
    let (mut w, mut g) = (World::new(), Governor::default());
    let r = flip_row(&mut w, &net, &mut g);
    assert!(r.verdicts.iter().all(|v| matches!(v, Verdict::Failed(_))), "{:?}", r.verdicts);
    assert_eq!(r.max_tries, 3);
    assert!(ghosts(&w).is_empty());
    // A refusal is echoed: each try ends at the echo (4 ticks), not at the timeout (12).
    assert!(r.ticks < 20 * 3 * 6 + 200, "{} ticks", r.ticks);
}

#[test]
fn a_cell_changed_otherwise_fails_at_once() {
    let w = World::new();
    let mut c = Confirm::new(Expect::Gone, Some(w.stone), 0, 12, 3);
    let dirt = w.registry.states_of("minecraft:dirt").next().expect("dirt");
    assert_eq!(c.check(1, Some(dirt), false), Verdict::Wait, "before the deadline anything may still come");
    assert!(matches!(c.check(12, Some(dirt), false), Verdict::Failed(why) if why.contains("dirt")));
}

#[test]
fn a_predicted_block_counts_only_once_the_server_echoes_it() {
    let w = World::new();
    let stone = w.registry.states_of("minecraft:stone").next().expect("stone");
    let mut c = Confirm::new(Expect::Solid, Some(w.air), 0, 12, 3).predicted();
    assert_eq!(c.check(1, Some(stone), false), Verdict::Wait, "our own prediction is no confirmation");
    assert_eq!(c.check(12, Some(stone), false), Verdict::Retry, "no echo by the deadline: as unchanged");
    c.retried(12, 12);
    assert_eq!(c.check(15, Some(stone), true), Verdict::Done);
}

#[test]
fn a_refused_placement_gives_the_block_back() {
    let block = ItemStack { network_id: 5, count: 10, block_runtime_id: 3, ..ItemStack::default() };
    let left = ItemStack { count: 9, ..block.clone() };
    assert_eq!(unpredict(&left, &left, &block), Some(block.clone()), "slot still as predicted: undo");
    let server = ItemStack { count: 7, ..block.clone() };
    assert_eq!(unpredict(&server, &left, &block), None, "the server set the slot since: keep its stack");
}
