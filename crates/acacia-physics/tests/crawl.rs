//! Crawling in a one-block gap, against strict BDS 1.26.52.3 (Ferrock's sessions of 2026-10-27).

use acacia_physics::test_world::{TestBlock, TestWorld};
use acacia_physics::{Input, PlayerState, tick};

/// A crawling player at the start of a one-high tunnel along +z, its floor at y 149.
fn crawling_in(gap: Option<TestBlock>) -> (TestWorld, PlayerState) {
    let mut w = TestWorld::new();
    w.fill(Some(TestBlock::Stone), [-2, 149, -2], [2, 151, 40]);
    w.fill(gap, [0, 150, 0], [0, 150, 40]);
    let mut st = PlayerState::new([0.5, 150.0, 0.5]);
    st.on_ground = true;
    st.crawling = true;
    st.size[1] = st.crawling_height;
    (w, st)
}

#[test]
fn a_crawl_in_water_moves_at_the_full_pace() {
    // Teleported into a tunnel of still water and sprinting toward +z: the server's feet on the
    // first three inputs, which a crawl's slowdown left 0.0137 behind on each.
    let (w, mut st) = crawling_in(TestBlock::from_kind("water"));
    let input = Input { move_vector: [0.0, 1.0], sprint: true, ..Input::default() };
    for z in [0.5196, 0.55682, 0.60995] {
        tick(&mut st, &input, &w);
        assert!((st.pos[2] - z).abs() < 1e-4, "{:?}, the server's z {z}", st.pos);
    }
    assert!(st.crawling, "still crawling");
}

#[test]
fn a_crawl_on_dry_ground_does_not_sprint() {
    // Teleported into a dry tunnel and holding sprint: the server's feet on the first three
    // inputs, which a sprint's speed left ahead by 0.0088 on each.
    let (w, mut st) = crawling_in(None);
    let input = Input { move_vector: [0.0, 1.0], sprint: true, ..Input::default() };
    for z in [0.5294, 0.57483, 0.62906] {
        tick(&mut st, &input, &w);
        assert!((st.pos[2] - z).abs() < 1e-4, "{:?}, the server's z {z}", st.pos);
        assert!(!st.sprinting, "sprinting");
    }
}
