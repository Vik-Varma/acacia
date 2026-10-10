//! The lever, which the pack gives one image and no shape: Java's `block/lever` model, a
//! cobblestone plate with the handle tilted 45° one way or, thrown, the other.

use std::sync::Arc;

use glam::{Mat3, Vec3};

use super::shape::ModelFace;
use super::{Material, Tint};

/// The handle's foot, which it tilts about.
const PIVOT: Vec3 = Vec3::new(8.0, 1.0, 8.0);

/// One face from its corners 0, 1 and 3 (counter-clockwise from outside) and the texels
/// `[u0, v0, u1, v1]` it shows, corner 0 at its bottom left.
fn face(corners: [Vec3; 3], [u0, v0, u1, v1]: [f32; 4], texture: u16, material: Material) -> ModelFace {
    ModelFace {
        corners: corners.map(Vec3::to_array),
        uv: [[u0, v1], [u1, v1], [u0, v0]],
        texture,
        tint: Tint::None,
        material,
        shade: None,
        ambient_occlusion: false,
        cull: None,
    }
}

/// A box's six faces. `side` and `top` are the texels of its sides and of its top and bottom.
fn cuboid(a: Vec3, b: Vec3, side: [f32; 4], top: [f32; 4], texture: u16, material: Material) -> [ModelFace; 6] {
    let p = Vec3::new;
    [
        ([p(a.x, b.y, b.z), p(b.x, b.y, b.z), p(a.x, b.y, a.z)], top),
        ([p(a.x, a.y, a.z), p(b.x, a.y, a.z), p(a.x, a.y, b.z)], top),
        ([p(a.x, a.y, b.z), p(b.x, a.y, b.z), p(a.x, b.y, b.z)], side),
        ([p(b.x, a.y, a.z), p(a.x, a.y, a.z), p(b.x, b.y, a.z)], side),
        ([p(b.x, a.y, b.z), p(b.x, a.y, a.z), p(b.x, b.y, b.z)], side),
        ([p(a.x, a.y, a.z), p(a.x, a.y, b.z), p(a.x, b.y, a.z)], side),
    ]
    .map(|(corners, texels)| face(corners, texels, texture, material))
}

/// A point of the lever standing on a floor (leaning along z) to where `lever_direction` has
/// it, and whether that mirrors it.
fn turned(direction: &str, [x, y, z]: [f32; 3]) -> ([f32; 3], bool) {
    match direction {
        "north" => ([x, z, 16.0 - y], false),
        "south" => ([x, z, y], true),
        "east" => ([y, z, x], false),
        "west" => ([16.0 - y, z, x], true),
        "up_east_west" => ([z, y, x], true),
        "down_east_west" => ([z, 16.0 - y, x], false),
        "down_north_south" => ([x, 16.0 - y, z], true),
        _ => ([x, y, z], false),
    }
}

/// `plate` and `handle` are the atlas layers of cobblestone and of the lever's image.
pub(super) fn faces(direction: &str, thrown: bool, plate: u16, handle: u16) -> Arc<[ModelFace]> {
    let base = cuboid(Vec3::new(5.0, 0.0, 4.0), Vec3::new(11.0, 3.0, 12.0), [5.0, 13.0, 11.0, 16.0], [5.0, 4.0, 11.0, 12.0], plate, Material::Opaque);
    let tilt = Mat3::from_rotation_x(if thrown { 45f32 } else { -45f32 }.to_radians());
    let stick = cuboid(Vec3::new(7.0, 1.0, 7.0), Vec3::new(9.0, 11.0, 9.0), [7.0, 6.0, 9.0, 16.0], [7.0, 6.0, 9.0, 8.0], handle, Material::Cutout);
    let tilted = stick.map(|f| ModelFace { corners: f.corners.map(|c| (PIVOT + tilt * (Vec3::from(c) - PIVOT)).to_array()), ..f });
    let placed = |f: ModelFace| {
        let [c0, c1, c3] = f.corners.map(|c| turned(direction, c).0);
        // A mirrored face is wound the other way: its corners 1 and 3 change places.
        match turned(direction, [0.0; 3]).1 {
            true => ModelFace { corners: [c0, c3, c1], uv: [f.uv[0], f.uv[2], f.uv[1]], ..f },
            false => ModelFace { corners: [c0, c1, c3], ..f },
        }
    };
    base.into_iter().chain(tilted).map(placed).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(f: &ModelFace) -> Vec3 {
        let [c0, c1, c3] = f.corners.map(Vec3::from);
        (c1 - c0).cross(c3 - c0).normalize()
    }

    #[test]
    fn a_lever_keeps_its_faces_outward_wherever_it_is_put() {
        for direction in ["up_north_south", "up_east_west", "down_north_south", "down_east_west", "north", "south", "east", "west"] {
            let lever = faces(direction, false, 1, 2);
            assert_eq!(lever.len(), 12);
            // The plate's faces look away from its middle.
            let middle = lever[..6].iter().flat_map(|f| f.corners).map(Vec3::from).sum::<Vec3>() / 18.0;
            for f in &lever[..6] {
                assert!(normal(f).dot(Vec3::from(f.corners[0]) - middle) > 0.0, "{direction}: {f:?}");
            }
        }
        // On a north wall the plate is against the block to the south; thrown, the handle's end is high.
        let end = |thrown| faces("north", thrown, 1, 2)[6].corners[0];
        assert!(faces("north", false, 1, 2)[..6].iter().all(|f| f.corners.iter().all(|c| c[2] >= 13.0)));
        assert!(end(true)[1] > 8.0 && end(false)[1] < 8.0, "{:?} {:?}", end(true), end(false));
    }
}
