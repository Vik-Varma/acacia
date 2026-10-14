//! Ticks to the goal with macro moves on and off (plan step 9), both profiles, shortcuts off and on (the
//! navigator's default). Without shortcuts macros never lose and win clearly on flat ground (`Optimal`) and
//! over gaps. Shortcuts already sprint-jump flat ground, so with them on macros win over gaps and may lose a
//! few ticks of hop discretization elsewhere.
//! ~150 s in debug, so ignored by default:
//! `cargo test --release -p acacia-bot --lib macros_never_slow -- --ignored --nocapture`.

use acacia_physics::{BlockPos, Vec3};

use super::drive::{PROFILES, macro_opts, plan_and_follow};
use super::grid::Grid;
use crate::pathfind::MovementProfile::{self, Human, Optimal};
use crate::pathfind::SearchOpts;

struct Course {
    name: &'static str,
    grid: fn() -> Grid,
    from: Vec3,
    goal: BlockPos,
}

fn flat() -> Grid {
    Grid::floor(-31, 46)
}

/// A one-gap then a two-gap (both within the classic gap jump), on a 5-wide lane.
fn gaps() -> Grid {
    let g = Grid::new();
    g.fill([-12, 0, -2], [0, 0, 2], "stone").fill([2, 0, -2], [6, 0, 2], "stone").fill([9, 0, -2], [24, 0, 2], "stone");
    g
}

/// A step up every three blocks, four steps.
fn stairs() -> Grid {
    let g = Grid::floor(-20, 40);
    for k in 1..=4 {
        g.fill([3 * k, k, -2], [40, k, 2], "stone");
    }
    g
}

const COURSES: [Course; 4] = [
    Course { name: "flat 40 straight", grid: flat, from: [-20.5, 1.0, 0.5], goal: [20, 1, 0] },
    Course { name: "diagonal 30x11", grid: flat, from: [-15.5, 1.0, -5.5], goal: [15, 1, 6] },
    Course { name: "gap course", grid: gaps, from: [-8.5, 1.0, 0.5], goal: [16, 1, 0] },
    Course { name: "stairs course", grid: stairs, from: [-10.5, 1.0, 0.5], goal: [20, 5, 0] },
];

/// Least ticks macros on must save: (course, profile, shortcuts, ticks). Set from the first run (saved 27, 28,
/// 21, 19, 19) with margin. `Human` hops do not pay on flat ground (`costs::HUMAN_JUMP`): no gain asked there.
const GAINS: [(&str, MovementProfile, bool, f32); 5] = [
    ("flat 40 straight", Optimal, false, 20.0),
    ("gap course", Optimal, false, 20.0),
    ("gap course", Human, false, 15.0),
    ("gap course", Optimal, true, 12.0),
    ("gap course", Human, true, 12.0),
];
/// Ticks macros may lose with shortcuts on where no gain is asked (measured: up to 3).
const SHORTCUT_SLACK: f32 = 4.0;

/// Mean ticks to the goal over the profile's execution seeds.
fn ticks(c: &Course, g: &Grid, opts: &SearchOpts, shortcuts: bool) -> f32 {
    let (_, runs) = plan_and_follow(g, c.from, c.goal, opts, shortcuts).unwrap_or_else(|| panic!("{}: no plan", c.name));
    runs.iter().map(|o| o.ticks as f32).sum::<f32>() / runs.len() as f32
}

#[test]
#[ignore = "slow in debug; run in release"]
fn macros_never_slow_the_bot_down() {
    let mut rows = Vec::new();
    for c in &COURSES {
        let g = (c.grid)();
        for profile in PROFILES {
            for shortcuts in [false, true] {
                let on = macro_opts(profile);
                let off = ticks(c, &g, &SearchOpts { macros: false, ..on.clone() }, shortcuts);
                rows.push((c.name, profile, shortcuts, off, ticks(c, &g, &on, shortcuts)));
            }
        }
    }
    eprintln!("SPEED {:<18} {:<8} {:<9} {:>6} {:>6} {:>6}", "course", "profile", "shortcuts", "off", "on", "saved");
    for &(name, profile, shortcuts, off, on) in &rows {
        eprintln!("SPEED {name:<18} {:<8} {shortcuts:<9} {off:>6.1} {on:>6.1} {:>6.1}", format!("{profile:?}"), off - on);
    }
    for &(name, profile, shortcuts, off, on) in &rows {
        let gain = GAINS.iter().find(|g| (g.0, g.1, g.2) == (name, profile, shortcuts)).map(|g| g.3);
        let most = match gain {
            Some(gain) => off - gain,
            None if shortcuts => off + SHORTCUT_SLACK,
            None => off,
        };
        assert!(on <= most, "{name} {profile:?} shortcuts {shortcuts}: {on} ticks with macros, {off} without (gain {gain:?})");
    }
}
