//! Live check of the schematic builder (acacia-pathfind/docs/builder.md): builds `tests/fixtures/build/
//! house16.mcstructure` on the pad from two barrel stations, with junk in the footprint to clear. The bot is
//! disconnected after `KILL_AT` placements (default 150; 0: never) and a fresh one resumes from the saved job.
//! Afterwards an observer bot compares every cell with the structure. Needs the test pack off.
//! `cargo run --release -p acacia-bot --example build -- <server> <name> <observer>`
mod support;

use std::time::Instant;

use acacia_bot::build::{check, BuildJob, BuildStatus, Builder, Phase, Structure, Transform};
use acacia_bot::pathfind::process::{BlockView, Status};
use acacia_bot::pathfind::Processes;
use acacia_bot::Bot;
use support::{Error, Pad, corrections};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/build/house16.mcstructure");
const MAX_TICKS: u32 = 40_000;
/// Footprint corner (pad offset).
const AT: [i32; 3] = [4, 0, -8];
const STATIONS: [[i32; 3]; 2] = [[0, 0, -10], [1, 0, -10]];
/// Stacks per barrel, as (item, stacks): enough for the house with spares.
const STOCK: [&[(&str, u32)]; 2] = [
    &[("cobblestone", 6), ("oak_planks", 4), ("oak_log", 1), ("glass_pane", 1), ("oak_slab", 2), ("oak_stairs", 1)],
    &[("torch", 1), ("ladder", 1), ("oak_fence", 1), ("cobblestone_wall", 1), ("stone_button", 1), ("furnace", 1), ("dispenser", 1), ("observer", 1), ("piston", 1)],
];

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).try_init();
    let mut args = std::env::args().skip(1);
    let server = args.next().unwrap_or_else(|| "127.0.0.1:19170".into());
    let name = args.next().unwrap_or_else(|| "Builder".into());
    let observer = args.next().unwrap_or_else(|| "BuildObs".into());
    let kill_at: u32 = env("KILL_AT", 150);
    let (mut a, pad) = support::connect(&server, &name).await?;
    let junk = [([6, 0, -6], [8, 1, -4], "dirt"), ([12, 2, 2], [13, 2, 3], "stone")];
    pad.build(&mut a, &junk, 5).await?;
    stock(&mut a, &pad).await?;
    for c in ["/clear @s", "/give @s diamond_pickaxe", "/give @s dirt 64"] {
        pad.run(&a, c);
        a.wait_ticks(4).await?;
    }
    pad.teleport(&mut a, [0.5, 0.0, 0.5], 180.0).await?;
    let mut job = BuildJob::new(FIXTURE, Transform { origin: pad.block(AT), ..Transform::default() });
    job.stations = STATIONS.iter().map(|&s| pad.block(s)).collect();
    let saved = job.to_json();
    let started = Instant::now();
    let (first, mut fixes) = run(&mut a, job, kill_at).await?;
    let mut total = first.clone();
    if kill_at > 0 && first.1.phase != Phase::Done {
        println!("killed after {} ticks: {:?}", first.0, first.1);
        a.disconnect().await;
        let (mut b, _) = support::connect(&server, &name).await?;
        let (second, more) = run(&mut b, BuildJob::from_json(&saved)?, 0).await?;
        fixes += more;
        total = (first.0 + second.0, BuildStatus { placed: first.1.placed + second.1.placed, broken: first.1.broken + second.1.broken, restocks: first.1.restocks + second.1.restocks, ..second.1 });
        pad.go_home(&mut b).await?;
        b.disconnect().await;
    } else {
        pad.go_home(&mut a).await?;
        a.disconnect().await;
    }
    let structure = Structure::load(&std::fs::read(FIXTURE)?)?;
    let at = BuildJob::from_json(&saved)?.at;
    let (mut o, _) = support::connect(&server, &observer).await?;
    let spot = pad.block([12, 9, 0]);
    pad.run(&o, &format!("/tp @s {} {} {}", spot[0] as f32 + 0.5, spot[1], spot[2] as f32 + 0.5));
    o.wait_ticks(80).await?;
    let wrong = check(&structure, &at, |p| view(&o, p));
    o.disconnect().await;
    let (ticks, s) = total;
    println!(
        "{} after {ticks} ticks ({:.0} s): {} placed, {} broken, {} restocks, {} corrections, {} mismatches, unsupported {:?}, failed {:?}, scaffold {}",
        if wrong.is_empty() && s.phase == Phase::Done { "PASS" } else { "FAIL" },
        started.elapsed().as_secs_f32(), s.placed, s.broken, s.restocks, fixes, wrong.len(), s.unsupported, s.failed, s.scaffold
    );
    wrong.iter().take(20).for_each(|m| println!("  {:?}: want {} found {}", m.pos, m.want, m.found));
    Ok(())
}

fn view(bot: &Bot, p: [i32; 3]) -> Option<BlockView> {
    bot.block_state(p).map(BlockView::of)
}

/// Places the barrels and fills them.
async fn stock(a: &mut Bot, pad: &Pad) -> Result<(), Error> {
    for (station, items) in STATIONS.iter().zip(STOCK) {
        let [x, y, z] = pad.block(*station);
        pad.run(a, &format!("/setblock {x} {y} {z} barrel"));
        a.wait_ticks(2).await?;
        let mut slot = 0;
        for (item, stacks) in items {
            for _ in 0..*stacks {
                pad.run(a, &format!("/replaceitem block {x} {y} {z} slot.container {slot} {item} 64"));
                a.wait_ticks(2).await?;
                slot += 1;
            }
        }
    }
    Ok(())
}

/// Builds until done, `kill_at` placements, or the tick cap; returns ticks, the last status and corrections.
async fn run(bot: &mut Bot, job: BuildJob, kill_at: u32) -> Result<((u32, BuildStatus), u32), Error> {
    let before = corrections(bot);
    let diag = Builder::new(job.clone())?;
    let (mut last_done, mut still) = (0, 0);
    let builder = Builder::new(job)?;
    let status = builder.status();
    let mut procs = Processes::new();
    let id = procs.add(builder, 10);
    let mut ticks = 0;
    loop {
        let st = procs.tick(bot).await?;
        ticks += 1;
        let s = status.lock().expect("status").clone();
        if ticks % 400 == 0 {
            println!("tick {ticks}: {:?} done {}/{} place {} break {} placed {} restocks {} missing {:?}", s.phase, s.done, s.cells, s.to_place, s.to_break, s.placed, s.restocks, s.missing);
        }
        (still, last_done) = if s.done == last_done { (still + 1, last_done) } else { (0, s.done) };
        if still > 0 && still % 1000 == 0 {
            let m = bot.movement();
            println!("STALL at {:?} ground {:?} status {st:?} failed {:?}", m.and_then(|m| m.position()), m.map(|m| m.on_ground()), s.failed);
            diag.explain_for(bot).iter().take(12).for_each(|l| println!("  OPEN {l}"));
        }
        if st == Status::Empty || procs.outcome(id).is_some() {
            println!("outcome {:?}", procs.outcome(id));
            let mut s = s;
            s.phase = if procs.outcome(id) == Some(&Ok(())) { Phase::Done } else { s.phase };
            return Ok(((ticks, s), corrections(bot) - before));
        }
        if (kill_at > 0 && s.placed >= kill_at) || ticks >= MAX_TICKS || still > 3000 {
            return Ok(((ticks, s), corrections(bot) - before));
        }
    }
}
