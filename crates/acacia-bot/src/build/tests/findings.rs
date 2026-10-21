use std::io::Write;
use std::sync::{Arc, Mutex};

use crate::build::findings::{biography, findings, summary};
use crate::build::journal::{Entry, Journal, Kind, Line, Want, WalkEnd};
use crate::build::{HoldWhy, Phase};

#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl Write for Buf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn parse(bufs: &[Buf]) -> Vec<Line> {
    let mut lines: Vec<Line> = bufs.iter().flat_map(|b| String::from_utf8(b.0.lock().unwrap().clone()).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect::<Vec<_>>()).collect();
    lines.sort_by_key(|l| (l.t, l.tick));
    lines
}

fn hold(why: HoldWhy, claimed: usize) -> Entry {
    Entry::Tick { feet: [0.0; 3], want: Want::Hold, phase: Phase::Building, why: Some(why), cell: None, saw: None, claimed, taken: 2, to_place: 1 }
}

fn placed(cell: [i32; 3]) -> Entry {
    Entry::Acted { kind: Kind::Place, cell: Some(cell), item: Some("oak_slab".into()), target: Some("minecraft:oak_slab".into()), err: None }
}

#[test]
fn a_second_placement_into_a_cell_is_a_double_place() {
    let (a, b) = (Buf::default(), Buf::default());
    let (ja, jb) = (Journal::new("a", Box::new(a.clone()), false), Journal::new("b", Box::new(b.clone()), false));
    ja.write(1, placed([1, 2, 3]));
    jb.write(2, placed([1, 2, 3]));
    ja.write(3, placed([4, 5, 6]));
    ja.write(4, Entry::Acted { kind: Kind::Break, cell: Some([4, 5, 6]), item: None, target: None, err: None });
    ja.write(5, placed([4, 5, 6]));
    let double = || Entry::Acted { kind: Kind::Place, cell: Some([7, 0, 0]), item: Some("oak_slab".into()), target: Some("minecraft:oak_double_slab".into()), err: None };
    ja.write(6, double());
    ja.write(7, double());
    let lines = parse(&[a, b]);
    let found = findings(&lines);
    assert_eq!(found.iter().filter(|f| f.starts_with("DOUBLE_PLACE")).count(), 1, "{found:?}");
    assert!(found[0].contains("[1, 2, 3]"), "{found:?}");
    assert_eq!(biography(&lines, [1, 2, 3]).len(), 2);
    assert_eq!(biography(&lines, [4, 5, 6]).len(), 3);
}

#[test]
fn holds_with_claims_are_long_holds_and_stale_claims() {
    let a = Buf::default();
    let j = Journal::new("a", Box::new(a.clone()), false);
    (0..150).for_each(|t| j.write(t, hold(HoldWhy::Blocked, 2)));
    j.write(150, Entry::Tick { feet: [0.0; 3], want: Want::Act, phase: Phase::Building, why: None, cell: Some([0, 0, 0]), saw: None, claimed: 1, taken: 0, to_place: 1 });
    let lines = parse(&[a]);
    let found = findings(&lines);
    assert!(found.iter().any(|f| f.starts_with("LONG_HOLD a 150 ticks")), "{found:?}");
    assert!(found.iter().any(|f| f.starts_with("STALE_CLAIM a")), "{found:?}");
    assert!(summary(&lines)[0].contains("hold/Blocked 7.5s"), "{:?}", summary(&lines));
}

#[test]
fn walks_are_timed_and_replaced_walks_counted() {
    let a = Buf::default();
    let j = Journal::new("a", Box::new(a.clone()), false);
    j.walk_start(0, [0.0; 3], [5, 0, 0], vec![[4, 0, 0]]);
    j.walk_start(4, [0.0; 3], [9, 0, 0], vec![[8, 0, 0]]);
    j.write(20, Entry::Tick { feet: [3.0, 0.0, 4.0], want: Want::Go, phase: Phase::Building, why: None, cell: Some([9, 0, 0]), saw: None, claimed: 0, taken: 0, to_place: 1 });
    j.walk_end(20, WalkEnd::Arrived);
    let lines = parse(&[a]);
    let ends: Vec<_> = lines.iter().filter_map(|l| if let Entry::WalkEnd { end, ticks, dist, acts, .. } = &l.e { Some((*end, *ticks, *dist, *acts)) } else { None }).collect();
    assert_eq!(ends, vec![(WalkEnd::Switched, 4, 0.0, 0), (WalkEnd::Arrived, 16, 5.0, 0)]);
    assert!(findings(&lines).iter().any(|f| f.starts_with("GOAL_THRASH a 1")));
}
