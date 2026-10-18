use crate::build::{LocalBoard, TaskBoard};

#[test]
fn a_region_is_never_claimed_twice() {
    let b = LocalBoard::manual(100);
    assert_eq!(b.claim("a", &[(0, 0), (1, 0)]), Some((0, 0)));
    assert_eq!(b.claim("b", &[(0, 0), (1, 0)]), Some((1, 0)));
    assert_eq!(b.claim("c", &[(0, 0), (1, 0)]), None);
    assert_eq!(b.claim("a", &[(0, 0)]), Some((0, 0)), "renewing its own claim");
    b.release("a");
    assert_eq!(b.claim("c", &[(0, 0), (1, 0)]), Some((0, 0)));
}

#[test]
fn a_lease_without_heartbeats_expires() {
    let b = LocalBoard::manual(100);
    b.claim("a", &[(0, 0)]);
    b.claim("b", &[(1, 0)]);
    b.set_now(60);
    assert_eq!(b.heartbeat("a", [0.0; 3]), Some((0, 0)));
    b.set_now(120);
    assert_eq!(b.taken("a"), Default::default(), "b's lease ran out");
    assert_eq!(b.claim("a", &[(1, 0)]), Some((1, 0)));
    assert_eq!(b.taken("b"), [(1, 0)].into());
}

#[test]
fn one_bot_at_a_time_opens_a_station() {
    let b = LocalBoard::manual(100);
    b.heartbeat("a", [0.0; 3]);
    assert!(b.open_station("a", [1, 2, 3]));
    assert!(!b.open_station("b", [1, 2, 3]));
    b.close_station("a", [1, 2, 3]);
    assert!(b.open_station("b", [1, 2, 3]));
    b.set_now(150);
    assert!(b.open_station("a", [1, 2, 3]), "b's hold ran out");
}
