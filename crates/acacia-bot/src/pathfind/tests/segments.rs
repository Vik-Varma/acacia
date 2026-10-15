//! Long gotos over several segments (milestone C1): the next segment is planned ahead and spliced on, so the
//! bot runs through the joins. The 500-block runs take minutes in debug, so they are ignored by default:
//! `cargo test --release -p acacia-bot --lib tests::segments -- --ignored --nocapture`.

use super::drive::{PROFILES, macro_opts, standing};
use super::grid::Grid;
use super::pilot_sim::{Sky, fly};
use crate::pathfind::{Goal, GotoOpts, MovementProfile};

/// Slower than this (blocks/tick) near a join counts as stopping there (walking is 0.216).
const MIN_JOIN_SPEED: f32 = 0.15;
/// Ticks either side of passing a join that are checked.
const JOIN_WINDOW: usize = 10;

/// A stone lane 7 wide along +x from x -5 to `len`, feet at y 1: two-block gaps at 100 and 300, a raised
/// stretch (one block up) from 200 to 230.
fn lane(len: i32) -> Grid {
    let g = Grid::sized(len / 16 + 2);
    g.fill([-5, 0, -3], [len, 0, 3], "stone");
    for x in [100, 300].into_iter().filter(|&x| x < len) {
        g.fill([x, 0, -3], [x + 1, 0, 3], "air");
    }
    if len > 230 {
        g.fill([200, 1, -3], [230, 1, 3], "stone");
    }
    g
}

fn opts(profile: MovementProfile) -> GotoOpts {
    GotoOpts { search: macro_opts(profile), seed: Some(1), ..GotoOpts::default() }
}

#[test]
#[ignore = "slow in debug; run in release"]
fn long_goto_runs_through_the_joins() {
    let g = lane(505);
    let goal = Goal::Block([500, 1, 0]);
    let mut bad = Vec::new();
    for profile in PROFILES {
        let sky = Sky { horizon: Some(5), delay: 0, max_ticks: 4000 };
        let f = fly(&g, &mut standing([0.5, 1.0, 0.5]), goal.clone(), opts(profile), sky);
        let speeds = f.join_speeds(JOIN_WINDOW);
        eprintln!("JOINS {profile:?}: {} ticks, {} joins, {} searches, {} diverged, slowest near each join {speeds:.3?}", f.ticks, f.joins.len(), f.searches, f.divergences);
        let ok = f.result.is_ok() && f.joins.len() >= 3 && f.replans == 0 && speeds.iter().all(|&s| s > MIN_JOIN_SPEED);
        if !ok {
            bad.push(format!("{profile:?}: {:?}, {} joins, {} re-plans, speeds {speeds:?}", f.result, f.joins.len(), f.replans));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// A plan-ahead answering after the bot reached the segment's end: it waits there, then carries on.
#[test]
#[ignore = "slow in debug; run in release"]
fn late_splice_waits_at_the_join() {
    let g = lane(150);
    let sky = Sky { horizon: Some(3), delay: 140, max_ticks: 3000 };
    let f = fly(&g, &mut standing([0.5, 1.0, 0.5]), Goal::Block([145, 1, 0]), opts(MovementProfile::Optimal), sky);
    eprintln!("LATE {} ticks, joins {:?}, speeds {:.3?}", f.ticks, f.joins, f.join_speeds(JOIN_WINDOW));
    assert_eq!(f.result, Ok(()));
    assert!(!f.joins.is_empty() && f.replans == 0);
}

/// Without a horizon the whole lane is one search: the same goal, no joins (the baseline the joins are run against).
#[test]
#[ignore = "slow in debug; run in release"]
fn short_goto_has_no_joins() {
    let g = lane(60);
    let sky = Sky { horizon: None, delay: 0, max_ticks: 600 };
    let f = fly(&g, &mut standing([0.5, 1.0, 0.5]), Goal::Block([55, 1, 0]), opts(MovementProfile::Optimal), sky);
    assert_eq!((f.result, f.joins.len()), (Ok(()), 0));
}
