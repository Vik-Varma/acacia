//! Live check of server-confirmed actions (acacia-pathfind/docs/confirmed-actions.md) under injected lag, PASS/FAIL
//! per course: `dig` tunnels through stone across the pad, `bridge` crosses a gap in it. Afterwards an observer bot
//! (no lag, its own terrain) loads the course and every cell is compared with the lagging bot's world: a cell
//! that differs is a ghost. Path work issued twice at the same cell counts as a loop. Needs the test pack off.
//! `cargo run -p acacia-bot --example confirmed -- <server> <name> <observer>`; `ONLY=dig|bridge`;
//! `LAG_MS` (one-way inbound delay, default 150), `LOSS` (default 0.05: a 300 ms resend spike).
mod support;

use std::time::Duration;

use acacia_bot::pathfind::{Goal, GotoOpts, NavStatus, Navigator};
use acacia_bot::{Bot, BotConfig, Lag};
use support::{Error, Fill, Pad, corrections};

const MAX_TICKS: u32 = 3000;
/// Course cells compared: across the pad, floor to well above the tunnel.
const BOX: ([i32; 3], [i32; 3]) = ([-1, -3, -4], [16, 4, 4]);

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).try_init();
    let mut args = std::env::args().skip(1);
    let server = args.next().unwrap_or_else(|| "127.0.0.1:19170".into());
    let name = args.next().unwrap_or_else(|| "LagA".into());
    let observer = args.next().unwrap_or_else(|| "LagObs".into());
    let only = std::env::var("ONLY").ok();
    let lag = Lag { delay: Duration::from_millis(env("LAG_MS", 150)), jitter: Duration::from_millis(20), loss: env("LOSS", 0.05), spike: Duration::from_millis(300), seed: 7 };
    println!("lag {lag:?}");
    let (mut a, pad) = support::connect_config(&server, &name, BotConfig { lag: Some(lag), ..BotConfig::default() }).await?;
    let mut failed = 0;
    for course in ["dig", "bridge"] {
        if only.as_deref().is_some_and(|o| o != course) {
            continue;
        }
        let result = run(&mut a, &pad, course, &server, &observer).await;
        match result {
            Ok(summary) => println!("PASS {course}: {summary}"),
            Err(e) => {
                failed += 1;
                println!("FAIL {course}: {e}");
            }
        }
    }
    println!("{failed} failed");
    pad.go_home(&mut a).await?;
    a.disconnect().await;
    Ok(())
}

async fn run(a: &mut Bot, pad: &Pad, course: &str, server: &str, observer: &str) -> Result<String, Error> {
    let mut opts = GotoOpts::default();
    let fills: Vec<Fill> = if course == "dig" {
        opts.search.allow_dig = true;
        vec![([2, 0, support::PAD_MIN[2]], [9, 9, support::PAD_MAX[2]], "stone")]
    } else {
        opts.search.allow_bridge = true;
        vec![([4, support::PAD_MIN[1], support::PAD_MIN[2]], [10, -1, support::PAD_MAX[2]], "air")]
    };
    pad.build(a, &fills, 5).await?;
    for c in ["/clear @s", "/give @s diamond_pickaxe", "/give @s cobblestone 64"] {
        pad.run(a, c);
        a.wait_ticks(4).await?;
    }
    pad.teleport(a, [0.5, 0.0, 0.5], -90.0).await?;
    let (before, sent, retries) = (corrections(a), a.governor().actions_sent(), a.governor().retries());
    let mut nav = Navigator::new(Goal::Block(pad.block([14, 0, 0])), opts);
    let (mut ticks, mut worked) = (0, 0);
    let arrived = loop {
        match nav.tick(a).await {
            Ok(NavStatus::Arrived) => break Ok(()),
            Ok(s) => worked += u32::from(s == NavStatus::Working),
            Err(e) => break Err(e),
        }
        ticks += 1;
        if ticks >= MAX_TICKS {
            break Err(acacia_bot::ActionError::Timeout);
        }
    };
    a.wait_ticks(40).await?;
    let ghosts = ghosts(a, pad, server, observer).await?;
    let g = a.governor();
    let summary = format!(
        "{ticks} ticks, {worked} work moves, {} actions ({} retries), echo rtt {:?}, {} corrections, {} setbacks held, {} repeated work steps, {} ghosts",
        g.actions_sent() - sent, g.retries() - retries, g.rtt(), corrections(a) - before, nav.pilot().settles(), nav.pilot().work_repeats(), ghosts.len()
    );
    arrived.map_err(|e| format!("goto: {e} ({summary})"))?;
    if worked == 0 {
        return Err(format!("never worked: the course was crossed without digging or bridging ({summary})").into());
    }
    if !ghosts.is_empty() || nav.pilot().work_repeats() > 0 {
        return Err(format!("{summary}; ghosts at {ghosts:?}").into());
    }
    Ok(summary)
}

/// Cells of the course where the lagging bot's world differs from a fresh observer's.
async fn ghosts(a: &mut Bot, pad: &Pad, server: &str, observer: &str) -> Result<Vec<String>, Error> {
    let (mut o, _) = support::connect_config(server, observer, BotConfig::default()).await?;
    let spot = pad.block([6, 2, 6]);
    pad.run(a, &format!("/tp {observer} {} {} {}", spot[0] as f32 + 0.5, spot[1], spot[2] as f32 + 0.5));
    o.wait_ticks(60).await?;
    let (lo, hi) = BOX;
    let mut out = Vec::new();
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let p = pad.block([x, y, z]);
                let (ours, theirs) = (a.block_name(p), o.block_name(p));
                if theirs.is_none() || ours != theirs {
                    out.push(format!("{p:?} bot {ours:?} server {theirs:?}"));
                }
            }
        }
    }
    o.disconnect().await;
    Ok(out)
}
