//! The rollout cache and budget (plan step 7): cached plans equal uncached ones, re-plans simulate
//! nothing, and a spent budget leaves classic moves. Bounds are counts, not times, so they hold in debug.

use super::drive::{PROFILES, macro_opts};
use super::perf::{Case, cases, run};
use crate::pathfind::{MovementProfile, Path, RolloutCache, SearchOpts};

fn case(name: &str) -> Case {
    cases().into_iter().find(|c| c.name == name).expect("perf case")
}

fn same_plan(a: &Path, b: &Path) {
    let cells = |p: &Path| p.nodes.iter().map(|n| (n.pos, n.kind)).collect::<Vec<_>>();
    assert_eq!((a.complete, cells(a)), (b.complete, cells(b)));
    assert!((a.cost - b.cost).abs() < 0.01, "{} vs {}", a.cost, b.cost);
}

#[test]
fn cached_plans_equal_uncached_and_replans_simulate_nothing() {
    for name in ["gap course Optimal", "gap course Human", "flat 80 Optimal"] {
        let c = case(name);
        let g = (c.grid)();
        let (fresh, _) = run(&c, &g, &mut RolloutCache::disabled());
        let mut cache = RolloutCache::default();
        let (cold, _) = run(&c, &g, &mut cache);
        let (replan, _) = run(&c, &(c.grid)(), &mut cache);
        let (f, s, r) = (fresh.macro_stats, cold.macro_stats, replan.macro_stats);
        eprintln!("{name}: uncached {f:?}\n  cold {s:?}\n  replan {r:?}");
        same_plan(&fresh, &cold);
        same_plan(&fresh, &replan);
        assert!(s.rollouts <= f.rollouts && s.hits > 0, "{name}: the cache saved nothing");
        assert_eq!((r.rollouts, r.misses), (0, 0), "{name}: a re-plan on unchanged terrain simulated");
    }
}

/// The perf guard (plan step 7): bounds on simulation per plan, as counts (measured: flat 485 rollouts and
/// half hits; Human's flat run 58 rollouts; the 5-gap refusal 214).
#[test]
fn rollouts_per_plan_stay_bounded() {
    let cases = [("flat 80 Optimal", 600, 0.4), ("flat 80 Human", 100, 0.9), ("diagonal 30x11 Human", 60, 0.9), ("5-gap refusal Human", 300, 0.7)];
    for (name, max_rollouts, min_hit_rate) in cases {
        let c = case(name);
        let (p, _) = run(&c, &(c.grid)(), &mut RolloutCache::default());
        let s = p.macro_stats;
        let rate = s.hits as f32 / (s.hits + s.misses).max(1) as f32;
        assert!(s.rollouts <= max_rollouts && rate >= min_hit_rate, "{name}: {s:?}");
        assert_eq!(p.complete, !name.contains("refusal"), "{name}");
    }
}

#[test]
fn a_spent_budget_leaves_classic_moves() {
    let c = case("flat 80 Optimal");
    let g = (c.grid)();
    for profile in PROFILES {
        let opts = SearchOpts { max_rollout_ticks: 300, ..macro_opts(profile) };
        let (p, _) = run(&Case { opts, ..case("flat 80 Optimal") }, &g, &mut RolloutCache::default());
        let s = p.macro_stats;
        assert!(p.complete && s.exhausted, "{profile:?}: {s:?}");
        // The budget is checked between moves: one move's solve may overrun it.
        assert!(s.ticks < 300 + 2_000, "{profile:?}: {s:?}");
    }
}

/// A cache filled for one profile is not used for another.
#[test]
fn cache_is_dropped_for_another_profile() {
    let c = case("gap course Optimal");
    let g = (c.grid)();
    let mut cache = RolloutCache::default();
    run(&c, &g, &mut cache);
    let human = Case { opts: macro_opts(MovementProfile::Human), ..case("gap course Optimal") };
    let (fresh, _) = run(&human, &g, &mut RolloutCache::default());
    let (p, _) = run(&human, &g, &mut cache);
    assert_eq!(p.macro_stats, fresh.macro_stats);
}
