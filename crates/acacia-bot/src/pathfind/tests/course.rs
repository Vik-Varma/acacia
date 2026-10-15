//! The parkour section of `examples/course.rs` in the headless simulation: its legs (gap3 to sprint40) run in
//! order from the exit of the classic section, as the example runs them on a server. ~80 s in release, so
//! ignored by default: `cargo test --release -p acacia-bot --lib tests::course -- --ignored --nocapture`.

use std::time::Duration;

use acacia_physics::BlockPos;

use super::drive::{PROFILES, Style, drive, macro_opts, plan, seeds, standing};
use super::grid::Grid;
use crate::pathfind::{Goal, SearchOpts};

#[allow(dead_code)]
#[path = "../../../examples/course/legs.rs"]
mod legs;

/// The course origin (the example's spawn feet block), placed so the section fits the grid with a chunk to
/// spare (movement stops next to an unloaded chunk).
const O: BlockPos = [-60, 5, 0];
/// Where the classic section's last leg ends.
const EXIT_X: i32 = 47;

fn at(p: BlockPos) -> BlockPos {
    [O[0] + p[0], O[1] + p[1], O[2] + p[2]]
}

/// The parkour section, with the end of the classic lane leading into it.
fn course() -> Grid {
    let g = Grid::new();
    g.fill(at([40, -4, -1]), at([EXIT_X, -1, 1]), "stone").fill(at([40, 0, -2]), at([EXIT_X, 5, -2]), "glass").fill(at([40, 0, 2]), at([EXIT_X, 5, 2]), "glass");
    for (a, b, block) in legs::lane(true).into_iter().chain(legs::LEGS.iter().filter(|l| l.far()).flat_map(|l| l.build.iter().copied())) {
        g.fill(at(a), at(b), block);
    }
    g
}

/// The navigator's search settings with half its time, as on a loaded server: every leg still plans in full. A
/// search cut short follows a partial path, and on this course those ended in a pit.
#[test]
#[ignore = "slow in debug; run in release"]
fn course_plans_complete_when_pressed_for_time() {
    let g = course();
    let mut incomplete = Vec::new();
    for profile in PROFILES {
        let opts = SearchOpts { profile, budget: Duration::from_secs(1), ..SearchOpts::default() };
        let mut from = at([EXIT_X, 0, 0]);
        for leg in legs::LEGS.iter().filter(|l| l.far()) {
            let goal = Goal::Block(at(leg.goal));
            let path = plan(&g, &standing([from[0] as f32 + 0.5, from[1] as f32, from[2] as f32 + 0.5]), &goal, &opts);
            if !path.complete {
                incomplete.push(format!("{profile:?} {}", leg.name));
            }
            from = at(leg.goal);
        }
    }
    assert!(incomplete.is_empty(), "plans cut short: {incomplete:?}");
}

/// Every parkour leg in order under each profile and seed, with no divergence; the bot rests 10 ticks between legs.
#[test]
#[ignore = "slow in debug; run in release"]
fn parkour_course_legs() {
    let g = course();
    let world = g.physics();
    let mut failed = Vec::new();
    for profile in PROFILES {
        let opts = macro_opts(profile);
        for &seed in seeds(profile) {
            let [x, y, z] = at([EXIT_X, 0, 0]);
            let mut m = standing([x as f32 + 0.5, y as f32, z as f32 + 0.5]);
            for leg in legs::LEGS.iter().filter(|l| l.far()) {
                let goal = Goal::Block(at(leg.goal));
                let path = plan(&g, &m, &goal, &opts);
                assert!(path.complete, "{profile:?} seed {seed} {}: no plan", leg.name);
                let style = Style { seed, shortcuts: true };
                let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drive(&g, &mut m, path.nodes, &goal, &opts, style, |_, _| {})));
                let outcome = run.as_ref().map_or("FAILED".to_string(), |o| format!("{} ticks, {} diverged", o.ticks, o.diverged));
                eprintln!("COURSE {profile:?} seed {seed} {:8} {outcome}", leg.name);
                if run.is_ok_and(|o| o.diverged == 0) {
                    m.controls.stop();
                    for _ in 0..10 {
                        m.tick(&world);
                    }
                } else {
                    failed.push(format!("{profile:?} seed {seed} {}", leg.name));
                    break;
                }
            }
        }
    }
    assert!(failed.is_empty(), "failed legs: {failed:?}");
}
