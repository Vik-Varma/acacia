//! The live builder setup (`examples/build.rs`, `examples/swarm_build.rs`): house16 on the pad, two stocked
//! barrels, junk in the footprint to clear, and an observer's cell-by-cell check afterwards.

use acacia_bot::build::{check, BuildJob, BuildStatus, Builder, Mismatch, Phase, Structure, Transform};
use acacia_bot::pathfind::process::{BlockView, Status};
use acacia_bot::pathfind::Processes;
use acacia_bot::Bot;

use super::{Error, Pad};

pub const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/build/house16.mcstructure");
const MAX_TICKS: u32 = 40_000;
/// Footprint corner (pad offset).
const AT: [i32; 3] = [4, 0, -8];
const STATIONS: [[i32; 3]; 2] = [[0, 0, -10], [1, 0, -10]];
/// Stacks per barrel, as (item, stacks): the house twice over. An offline bot that reconnects comes back with an
/// empty inventory (BDS 1.26.52, seen 2026-10-17), so what a dropped bot took is gone.
const STOCK: [&[(&str, u32)]; 2] = [
    &[("cobblestone", 9), ("oak_planks", 6), ("oak_log", 2), ("glass_pane", 2), ("oak_slab", 3), ("oak_stairs", 1)],
    &[("torch", 2), ("ladder", 2), ("oak_fence", 2), ("cobblestone_wall", 2), ("stone_button", 2), ("furnace", 2), ("dispenser", 2), ("observer", 2), ("piston", 2), ("oak_stairs", 1)],
];

/// Rebuilds the pad with junk in the footprint, stocks the barrels, and returns the job.
pub async fn setup(a: &mut Bot, pad: &Pad) -> Result<BuildJob, Error> {
    let junk = [([6, 0, -6], [8, 1, -4], "dirt"), ([12, 2, 2], [13, 2, 3], "stone")];
    pad.build(a, &junk, 5).await?;
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
    let mut job = BuildJob::new(FIXTURE, Transform { origin: pad.block(AT), ..Transform::default() });
    job.stations = STATIONS.iter().map(|&s| pad.block(s)).collect();
    Ok(job)
}

/// Empties the inventory, then a pickaxe and scaffold dirt.
pub async fn kit(bot: &mut Bot, pad: &Pad) -> Result<(), Error> {
    for c in ["/clear @s", "/give @s diamond_pickaxe", "/give @s dirt 64"] {
        pad.run(bot, c);
        bot.wait_ticks(4).await?;
    }
    Ok(())
}

/// Runs `builder` until done, `kill_at` placements (0: never), the tick cap or a long stall; returns ticks, the
/// last status and corrections. `label` prefixes the progress lines.
pub async fn drive(bot: &mut Bot, builder: Builder, kill_at: u32, label: &str) -> Result<(u32, BuildStatus, u32), Error> {
    let before = super::corrections(bot);
    let diag = Builder::new(builder.job().clone())?;
    let (mut last_done, mut still) = (0, 0);
    let status = builder.status();
    let mut procs = Processes::new();
    let id = procs.add(builder, 10);
    let mut ticks = 0;
    loop {
        let st = procs.tick(bot).await?;
        ticks += 1;
        let mut s = status.lock().expect("status").clone();
        if ticks % 400 == 0 {
            println!("{label} tick {ticks}: {:?} done {}/{} place {} break {} placed {} restocks {} missing {:?}", s.phase, s.done, s.cells, s.to_place, s.to_break, s.placed, s.restocks, s.missing);
        }
        (still, last_done) = if s.done == last_done { (still + 1, last_done) } else { (0, s.done) };
        if still > 0 && still % 1000 == 0 {
            let m = bot.movement();
            println!("{label} STALL at {:?} ground {:?} status {st:?} failed {:?}", m.and_then(|m| m.position()), m.map(|m| m.on_ground()), s.failed);
            diag.explain_for(bot).iter().take(12).for_each(|l| println!("  OPEN {l}"));
        }
        if st == Status::Empty || procs.outcome(id).is_some() {
            println!("{label} outcome {:?}", procs.outcome(id));
            if procs.outcome(id) == Some(&Ok(())) {
                s.phase = Phase::Done;
            }
            return Ok((ticks, s, super::corrections(bot) - before));
        }
        if (kill_at > 0 && s.placed >= kill_at) || ticks >= MAX_TICKS || still > 3000 {
            return Ok((ticks, s, super::corrections(bot) - before));
        }
    }
}

/// Connects `observer` over the house and compares every cell with the structure.
pub async fn observe(server: &str, observer: &str, pad: &Pad, job: &BuildJob) -> Result<Vec<Mismatch>, Error> {
    let structure = Structure::load(&std::fs::read(FIXTURE)?)?;
    let (mut o, _) = super::connect(server, observer).await?;
    let spot = pad.block([12, 9, 0]);
    pad.run(&o, &format!("/tp @s {} {} {}", spot[0] as f32 + 0.5, spot[1], spot[2] as f32 + 0.5));
    o.wait_ticks(80).await?;
    let wrong = check(&structure, &job.at, |p| o.block_state(p).map(BlockView::of));
    o.disconnect().await;
    Ok(wrong)
}
