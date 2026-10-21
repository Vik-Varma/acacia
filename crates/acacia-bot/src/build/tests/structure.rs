use crate::build::{Block, Structure};

#[test]
fn round_trips_through_mcstructure_bytes() {
    let mut s = Structure::new([3, 2, 4]);
    s.set([0, 0, 0], Some(Block::new("stone", "")));
    s.set([2, 1, 3], Some(Block::new("oak_stairs", "weirdo_direction=2,upside_down_bit=1")));
    s.set([1, 0, 2], Some(Block::new("air", "")));
    let back = Structure::load(&s.to_bytes()).expect("loads");
    assert_eq!(back, s);
    assert_eq!(back.get([2, 1, 3]).and_then(|b| b.prop("upside_down_bit")), Some("1"));
    assert_eq!(back.get([1, 1, 1]), None, "structure void");
    assert_eq!(back.cells().count(), 3);
}

#[test]
fn reads_cells_x_outermost_z_innermost() {
    let mut s = Structure::new([2, 2, 2]);
    s.set([1, 0, 0], Some(Block::new("stone", "")));
    let cells: Vec<_> = s.cells().map(|(p, _)| p).collect();
    assert_eq!(cells, vec![[1, 0, 0]]);
}
