//! Scaffolding against BDS 1.26.52.3 traces (Dataminer `out/1.26.52/movement-traces.json`, head
//! 58622b2: "scaffolding 9 high, forward and jump held from inside the bottom, wall behind").

use acacia_physics::test_world::{TestBlock, TestWorld};
use acacia_physics::{Input, PlayerState, tick};

/// A 9-high column at x 30, z 7 (y 200–208) on stone, with a stone wall behind it at z 8 up to y 209.
fn column() -> TestWorld {
    let mut w = TestWorld::new();
    w.fill(Some(TestBlock::Stone), [26, 199, 3], [34, 199, 11]);
    w.fill(TestBlock::from_kind("scaffolding"), [30, 200, 7], [30, 208, 7]);
    w.fill(Some(TestBlock::Stone), [30, 200, 8], [30, 209, 8]);
    w
}

#[test]
fn on_top_of_a_column_held_jump_jumps_after_the_climbs_delay() {
    let w = column();
    let mut st = PlayerState::new([30.5, 200.0, 7.5]);
    st.on_ground = true;
    let input = Input { move_vector: [0.0, 1.0], jump: true, ..Input::default() };
    let mut feet = Vec::new();
    for _ in 0..=72 {
        tick(&mut st, &input, &w);
        feet.push((st.pos[1], st.on_ground));
    }
    // The trace: past the top at tick 60, back down onto it at 64, six ticks there, then an ordinary
    // jump (+0.42) at 70.
    assert!((feet[60].0 - 209.14964).abs() < 1e-4, "climbing: {:?}", feet[60]);
    assert!((feet[64].0 - 209.00002).abs() < 1e-4 && feet[64].1, "landed on top: {:?}", feet[64]);
    assert!((feet[69].0 - 209.00002).abs() < 1e-4 && feet[69].1, "still standing: {:?}", feet[69]);
    assert!((feet[70].0 - 209.42001).abs() < 1e-4, "an ordinary jump, not a climb: {:?}", feet[70]);
    assert!((feet[71].0 - 209.75322).abs() < 1e-4, "{:?}", feet[71]);
}

#[test]
fn beside_a_column_held_jump_is_an_ordinary_jump() {
    // The trace "beside a scaffolding column, jump held": the player at (31.5, 200, 7.5), next to the
    // column without overlapping it.
    let w = column();
    let mut st = PlayerState::new([31.5, 200.0, 7.5]);
    st.on_ground = true;
    tick(&mut st, &Input { jump: true, ..Input::default() }, &w);
    assert!((st.pos[1] - 200.42).abs() < 1e-4, "{}", st.pos[1]);
}
