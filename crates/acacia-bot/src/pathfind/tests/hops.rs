//! Runs and hops (plan step 6): hop chains on flat ground, leaps chained from running momentum, the
//! heuristic's hop speed, and the hunger gate.

use super::drive::{assert_arrived, assert_exact, drive, macro_opts, plan, standing};
use super::grid::Grid;
use super::leaps::PROFILES;
use crate::movement::Controls;
use crate::pathfind::costs::HOP_SPEED;
use crate::pathfind::{Entry, Goal, GotoOpts, MoveKind, MovementProfile, Navigator, SearchOpts};

/// Distance covered after each tick of sprinting from standing, jumping whenever possible or never.
fn sprint_from_rest(jump: bool) -> Vec<f32> {
    let g = Grid::floor(-31, 46);
    let mut m = standing([-30.5, 1.0, 0.5]);
    (0..120)
        .map(|_| {
            m.controls = Controls { forward: 1.0, sprint: true, jump, yaw: -90.0, ..Controls::default() };
            m.tick(&g.physics());
            m.position().unwrap()[0] + 30.5
        })
        .collect()
}

#[test]
fn steady_hop_speed_bounds_the_heuristic() {
    let xs = sprint_from_rest(true);
    // Five whole 12-tick hops once the speed has settled.
    let v = (xs[119] - xs[59]) / 60.0;
    eprintln!("steady hop speed {v:.5} blocks/tick = {:.4} ticks/block", 1.0 / v);
    assert!((v - HOP_SPEED).abs() < 2e-4, "HOP_SPEED is {HOP_SPEED}, measured {v}");
    let opts = macro_opts(MovementProfile::Optimal);
    assert!(opts.speed().per_block <= 1.0 / v, "inadmissible");
    // From standing, no run beats the heuristic's startup either.
    for jump in [true, false] {
        for (t, &d) in sprint_from_rest(jump).iter().enumerate() {
            let h = opts.speed().run(d, true);
            assert!(h <= (t + 1) as f32, "jump {jump}, tick {t}: {d} blocks, h {h}");
        }
    }
}

/// Plans from standing at `from` and follows the plan: (plan, ticks taken).
fn plan_and_follow(g: &Grid, from: [f32; 3], goal: &Goal, opts: &SearchOpts) -> (crate::pathfind::Path, u32) {
    let mut m = standing(from);
    let p = plan(g, &m, goal, opts);
    assert!(p.complete, "{:?}", opts.profile);
    let o = drive(g, &mut m, p.nodes.clone(), goal, opts, |_, _| {});
    assert_eq!(o.diverged, 0, "{:?}", opts.profile);
    if opts.profile == MovementProfile::Optimal {
        assert_exact(&o);
    }
    (p, o.ticks)
}

/// A straight flat run: `Optimal` hops nearly all the way, faster than sprinting, and the heuristic stays
/// under the cost. `Human` pays a hesitation per jump, so short runs are not worth hopping; it only must
/// not do worse than sprinting.
#[test]
fn hops_along_flat_ground() {
    let g = Grid::floor(-31, 46);
    for (profile, to) in [(MovementProfile::Optimal, 30), (MovementProfile::Human, 0)] {
        let goal = Goal::Block([to, 1, 0]);
        let classic = SearchOpts { macros: false, ..macro_opts(profile) };
        let (_, sprinting) = plan_and_follow(&g, [-20.5, 1.0, 0.5], &goal, &classic);
        let opts = macro_opts(profile);
        let (p, ticks) = plan_and_follow(&g, [-20.5, 1.0, 0.5], &goal, &opts);
        let h = goal.estimate([-21, 1, 0], opts.speed(), true);
        let hops = p.nodes.iter().filter(|n| n.kind == MoveKind::Hop).count();
        eprintln!("{profile:?}: {ticks} ticks with {hops} hops, {sprinting} sprinting; h {h}, cost {}", p.cost);
        assert!(h <= p.cost && ticks <= sprinting, "{profile:?}");
        if profile == MovementProfile::Optimal {
            assert!(hops >= 5 && ticks < sprinting, "{:?}", p.nodes);
        }
    }
}

/// One-block stones two apart: each leap starts from the last one's momentum, with no stop between.
#[test]
fn chains_leaps_over_stepping_stones() {
    let g = Grid::new();
    g.fill([-8, 0, -1], [0, 0, 1], "stone").fill([15, 0, -1], [22, 0, 1], "stone");
    for x in [3, 6, 9, 12] {
        g.fill([x, 0, 0], [x, 0, 0], "stone");
    }
    let goal = Goal::Block([18, 1, 0]);
    for profile in PROFILES {
        let opts = macro_opts(profile);
        let mut m = standing([-3.5, 1.0, 0.5]);
        let p = plan(&g, &m, &goal, &opts);
        assert!(p.complete, "{profile:?}");
        let on_stones: Vec<_> = p.nodes.iter().filter(|n| (1..15).contains(&n.pos[0])).collect();
        assert!(on_stones.iter().all(|n| n.kind == MoveKind::Parkour && n.entry != Entry::Rest), "{profile:?}: {on_stones:?}");
        let o = drive(&g, &mut m, p.nodes, &goal, &opts, |_, _| {});
        assert_arrived(&o, [18, 1, 0]);
        assert_eq!(o.diverged, 0, "{profile:?}");
        if profile == MovementProfile::Optimal {
            assert_exact(&o);
        }
    }
}

#[test]
fn hunger_gate_drops_flat_hops_but_not_leaps() {
    let nav = Navigator::new(Goal::Block([0, 0, 0]), GotoOpts::default());
    assert!(!nav.search_opts(None, 6.0).hops && nav.search_opts(None, 6.5).hops);
    let g = super::leaps::gap_course(3);
    let opts = SearchOpts { hops: false, ..macro_opts(MovementProfile::Optimal) };
    let p = plan(&g, &standing([-10.5, 1.0, 0.5]), &Goal::Block([14, 1, 0]), &opts);
    assert!(p.complete && p.nodes.iter().all(|n| n.kind != MoveKind::Hop), "{:?}", p.nodes);
    assert!(p.nodes.iter().any(|n| n.kind == MoveKind::Parkour), "{:?}", p.nodes);
}
