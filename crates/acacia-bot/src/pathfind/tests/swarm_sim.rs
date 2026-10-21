//! Several builders sharing one job through a board, in one simulated world (see `build_sim.rs`).

use std::collections::HashMap;
use std::sync::Arc;

use acacia_physics::{BlockPos, Vec3};

use super::build_sim::{builder, items, run, step, World};
use super::build_tests::{scaffold_left, wrong, ORIGIN};
use super::grid::Grid;
use crate::build::{Block, Builder, LocalBoard, StatusHandle, Structure};
use crate::movement::Movement;
use crate::pathfind::process::{Arbiter, ProcId};

/// Board lease, in ticks.
const LEASE: u64 = 200;

struct SimBot {
    m: Movement,
    arb: Arbiter,
    id: ProcId,
    inv: HashMap<String, u32>,
    status: StatusHandle,
    /// The bot stops ticking (killed) from this tick on.
    stop_at: Option<u32>,
    /// Most of one item ever held.
    most: u32,
}

fn bot(b: Builder, status: StatusHandle, board: &Arc<LocalBoard>, who: &str, start: Vec3, inv: &[(&str, u32)]) -> SimBot {
    let mut arb = Arbiter::new();
    let id = arb.add(b.with_board(board.clone(), who), 10);
    let mut m = Movement::new();
    m.start(start, 0.0, 0.0);
    SimBot { m, arb, id, inv: items(inv), status, stop_at: None, most: 0 }
}

fn live(b: &SimBot, tick: u32) -> bool {
    b.stop_at.is_none_or(|t| tick < t)
}

/// Ticks the bots in turn (each with its own inventory) until every live one finishes; returns the ticks taken.
fn run_many(w: &mut World, bots: &mut [SimBot], board: &LocalBoard, max_ticks: u32) -> u32 {
    for tick in 0..max_ticks {
        board.set_now(tick as u64);
        for k in 0..bots.len() {
            if !live(&bots[k], tick) || bots[k].arb.outcome(bots[k].id).is_some() {
                continue;
            }
            w.others = bots.iter().enumerate().filter(|(j, o)| *j != k && live(o, tick)).filter_map(|(_, o)| o.m.position()).collect();
            let b = &mut bots[k];
            std::mem::swap(&mut w.inv, &mut b.inv);
            step(w, &mut b.m, &mut b.arb, tick);
            std::mem::swap(&mut w.inv, &mut b.inv);
            b.most = b.most.max(b.inv.values().copied().max().unwrap_or(0));
            b.m.tick(&w.g.physics());
        }
        if bots.iter().all(|b| !live(b, tick) || b.arb.outcome(b.id).is_some()) {
            return tick;
        }
    }
    max_ticks
}

fn plank_floor(side: i32) -> Structure {
    let mut s = Structure::new([side, 1, side]);
    (0..side).for_each(|x| (0..side).for_each(|z| s.set([x, 0, z], Some(Block::new("oak_planks", "")))));
    s
}

fn house16_items() -> Vec<(&'static str, u32)> {
    ["cobblestone", "oak_planks", "oak_log", "glass_pane", "oak_slab", "oak_stairs", "torch", "ladder", "oak_fence", "cobblestone_wall", "stone_button", "furnace", "dispenser", "observer", "piston", "dirt"].iter().map(|i| (*i, 400)).collect()
}

#[test]
#[ignore = "slow in debug: cargo test --release -- --ignored two_bots"]
fn two_bots_build_house16_in_well_under_one_bots_time() {
    let s = crate::build::tests::house16::house16();
    let inv = house16_items();
    let one = {
        let g = Grid::floor(-20, 40);
        let mut w = World::new(&g, &inv);
        let mut arb = Arbiter::new();
        let id = arb.add(builder(&s, ORIGIN, &[]).0, 10);
        run(&mut w, [0.5, 1.0, 0.5], &mut arb, id, 8000)
    };
    let g = Grid::floor(-20, 40);
    let mut w = World::new(&g, &[]);
    let board = Arc::new(LocalBoard::manual(LEASE));
    let mut bots: Vec<SimBot> = [("a", [0.5, 1.0, 0.5]), ("b", [2.5, 1.0, 0.5])]
        .into_iter()
        .map(|(who, start)| {
            let (b, st) = builder(&s, ORIGIN, &[]);
            bot(b, st, &board, who, start, &inv)
        })
        .collect();
    // Caps ~2.5x the expected ticks: a looping bot fails here in minutes, not after the testbox timeout.
    let two = run_many(&mut w, &mut bots, &board, 6000);
    for b in &bots {
        eprintln!("{:?}", b.status.lock().unwrap());
    }
    eprintln!("one bot {one} ticks, two bots {two} ticks: {:.2}", two as f32 / one as f32);
    assert!(bots.iter().all(|b| b.arb.outcome(b.id) == Some(&Ok(()))));
    assert_eq!(wrong(&g, &s), Vec::new());
    assert_eq!(scaffold_left(&g), Vec::<BlockPos>::new());
    // A regression bound, not the B2 bar: sim ticks are not live seconds (sim 0.41 against live 0.62 once, and
    // a pilot fix that sped one bot up moved it the other way). The bar is judged live: tools/build-ab.sh.
    assert!(two as f32 <= 0.7 * one as f32, "one {one} two {two}");
}

#[test]
#[ignore = "slow in debug: cargo test --release -- --ignored drops_after"]
fn house16_finishes_when_a_bot_drops_after_restocking() {
    // Live 2026-10-18: the kill run on share restock held forever after the drop.
    let s = crate::build::tests::house16::house16();
    let g = Grid::floor(-20, 40);
    let station = [-2, 1, -2];
    g.set_state(station, "barrel", "");
    let mut w = World::new(&g, &[]);
    w.stations.insert(station, house16_items().into_iter().map(|(i, n)| (format!("minecraft:{i}"), n * 2)).collect());
    let board = Arc::new(LocalBoard::manual(LEASE));
    let mut bots: Vec<SimBot> = [("a", [0.5, 1.0, 0.5]), ("b", [2.5, 1.0, 0.5])]
        .into_iter()
        .map(|(who, start)| {
            let (b, st) = builder(&s, ORIGIN, &[station]);
            bot(b, st, &board, who, start, &[("dirt", 64)])
        })
        .collect();
    bots[1].stop_at = Some(600);
    let ticks = run_many(&mut w, &mut bots, &board, 8000);
    let (a, b) = (bots[0].status.lock().unwrap().clone(), bots[1].status.lock().unwrap().clone());
    eprintln!("{ticks} ticks: a {a:?}\nb {b:?}");
    assert!(b.restocks > 0 && b.placed > 0, "b restocked and built before it dropped");
    assert_eq!(bots[0].arb.outcome(bots[0].id), Some(&Ok(())), "a {a:?}");
    assert_eq!(wrong(&g, &s), Vec::new());
}

#[test]
fn a_dropped_bots_claim_returns_to_the_pool() {
    let g = Grid::floor(-20, 30);
    let s = plank_floor(10);
    let mut w = World::new(&g, &[]);
    let board = Arc::new(LocalBoard::manual(LEASE));
    let mut bots: Vec<SimBot> = [("a", [0.5, 1.0, 0.5]), ("b", [13.5, 1.0, 13.5])]
        .into_iter()
        .map(|(who, start)| {
            let (b, st) = builder(&s, ORIGIN, &[]);
            bot(b, st, &board, who, start, &[("oak_planks", 200), ("dirt", 64)])
        })
        .collect();
    bots[1].stop_at = Some(80);
    let ticks = run_many(&mut w, &mut bots, &board, 6000);
    let (a, b) = (bots[0].status.lock().unwrap().clone(), bots[1].status.lock().unwrap().clone());
    eprintln!("{ticks} ticks: a {a:?}\nb {b:?}");
    assert!(b.placed > 0, "b worked before it dropped");
    assert_eq!(bots[0].arb.outcome(bots[0].id), Some(&Ok(())));
    assert_eq!(wrong(&g, &s), Vec::new());
}

#[test]
fn bots_restock_only_their_share_of_the_need() {
    let g = Grid::floor(-20, 30);
    let station = [0, 1, 0];
    g.set_state(station, "barrel", "");
    let s = plank_floor(16);
    let mut w = World::new(&g, &[]);
    // Exactly the floor (four stacks): a bot taking for the whole job would leave the other none. The first bot
    // reaches the barrel on its first tick, before the other has ticked: a share must count it all the same.
    w.stations.insert(station, [("minecraft:oak_planks".to_string(), 256)].into());
    let board = Arc::new(LocalBoard::manual(LEASE));
    let mut bots: Vec<SimBot> = [("a", [1.5, 1.0, 0.5]), ("b", [0.5, 1.0, 1.5])]
        .into_iter()
        .map(|(who, start)| {
            let (b, st) = builder(&s, ORIGIN, &[station]);
            bot(b, st, &board, who, start, &[("dirt", 64)])
        })
        .collect();
    let ticks = run_many(&mut w, &mut bots, &board, 20_000);
    let placed: Vec<u32> = bots.iter().map(|b| b.status.lock().unwrap().placed).collect();
    let most: Vec<u32> = bots.iter().map(|b| b.most).collect();
    eprintln!("{ticks} ticks, placed {placed:?}, most held {most:?}");
    assert!(bots.iter().all(|b| b.arb.outcome(b.id) == Some(&Ok(()))));
    assert_eq!(wrong(&g, &s), Vec::new());
    assert!(placed.iter().all(|p| *p > 0), "both bots built: {placed:?}");
    assert!(most.iter().all(|m| *m <= 128), "most held {most:?}");
}
