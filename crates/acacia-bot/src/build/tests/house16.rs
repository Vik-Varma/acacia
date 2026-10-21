//! The live builder fixture `tests/fixtures/build/house16.mcstructure` (16×8×16): a cobblestone house with a
//! plank floor crossed by logs, log corners, pane windows, a door gap, a slab rim, and inside stairs (both
//! halves), slabs (top, double), torches, a ladder, fences, walls, a button and four facing blocks.

use crate::build::{Block, Structure};

const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/build/house16.mcstructure");

fn put(s: &mut Structure, p: [i32; 3], name: &str, props: &str) {
    s.set(p, Some(Block::new(name, props)));
}

pub fn house16() -> Structure {
    let mut s = Structure::new([16, 8, 16]);
    let edge = |v: i32| v == 0 || v == 15;
    for x in 0..16 {
        for z in 0..16 {
            let border = edge(x) || edge(z);
            put(&mut s, [x, 0, z], if border { "cobblestone" } else { "oak_planks" }, "");
            for y in 1..6 {
                let (name, props) = match () {
                    _ if edge(x) && edge(z) => ("oak_log", "pillar_axis=y"),
                    _ if border && y == 5 => ("oak_slab", "minecraft:vertical_half=bottom"),
                    _ if border => ("cobblestone", ""),
                    _ => ("air", ""),
                };
                put(&mut s, [x, y, z], name, props);
            }
        }
    }
    for i in 1..15 {
        put(&mut s, [i, 0, 7], "oak_log", "pillar_axis=x");
        if i != 7 {
            put(&mut s, [7, 0, i], "oak_log", "pillar_axis=z");
        }
    }
    for w in [3, 4, 11, 12] {
        for y in 2..4 {
            for p in [[w, y, 0], [w, y, 15], [0, y, w], [15, y, w]] {
                put(&mut s, p, "glass_pane", "");
            }
        }
    }
    for x in 7..9 {
        for y in 1..3 {
            put(&mut s, [x, y, 15], "air", "");
        }
    }
    for x in 4..7 {
        put(&mut s, [x, 1, 1], "oak_stairs", "upside_down_bit=0,weirdo_direction=3");
    }
    // Against solid wall, clear of the windows: the only faces a high click can use.
    for z in 5..8 {
        put(&mut s, [14, 3, z], "oak_stairs", "upside_down_bit=1,weirdo_direction=0");
    }
    for z in 8..11 {
        put(&mut s, [1, 2, z], "oak_slab", "minecraft:vertical_half=top");
    }
    for x in 12..14 {
        put(&mut s, [x, 1, 12], "oak_double_slab", "minecraft:vertical_half=bottom");
    }
    for x in 3..6 {
        put(&mut s, [x, 1, 12], "oak_fence", "");
    }
    for x in 10..12 {
        put(&mut s, [x, 1, 3], "cobblestone_wall", "");
    }
    for y in 1..5 {
        put(&mut s, [8, y, 1], "ladder", "facing_direction=3");
    }
    put(&mut s, [1, 3, 7], "torch", "torch_facing_direction=west");
    put(&mut s, [14, 3, 9], "torch", "torch_facing_direction=east");
    put(&mut s, [5, 3, 1], "torch", "torch_facing_direction=north");
    put(&mut s, [1, 2, 6], "stone_button", "button_pressed_bit=0,facing_direction=5");
    put(&mut s, [2, 1, 2], "furnace", "minecraft:cardinal_direction=south");
    put(&mut s, [2, 1, 13], "dispenser", "facing_direction=2,triggered_bit=0");
    put(&mut s, [13, 1, 2], "observer", "minecraft:facing_direction=south,powered_bit=0");
    put(&mut s, [11, 1, 13], "piston", "facing_direction=4");
    s
}

#[test]
#[ignore = "regenerates the fixture"]
fn write_house16_fixture() {
    std::fs::create_dir_all(std::path::Path::new(PATH).parent().expect("dir")).expect("mkdir");
    std::fs::write(PATH, house16().to_bytes()).expect("write");
}

#[test]
fn house16_fixture_is_current() {
    let bytes = std::fs::read(PATH).expect("fixture: run write_house16_fixture");
    assert_eq!(Structure::load(&bytes).expect("loads"), house16());
}

#[test]
fn house16_is_all_buildable() {
    let s = house16();
    let blocked: Vec<_> = s.cells().filter(|(_, b)| !b.is_air() && crate::build::rules::item(b).is_none()).map(|(p, b)| (p, b.name.clone())).collect();
    assert_eq!(blocked, Vec::new());
}
