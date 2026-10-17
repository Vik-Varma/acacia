//! The builder in the headless simulation: the grid stands in for the server, placing with its own model of
//! how BDS sets a placed block's state (written from the place_probe table, apart from `build/rules.rs`).

use std::collections::HashMap;

use acacia_physics::{BlockPos, Vec3};
use acacia_world::BlockAccess;

use super::alter::blocks;
use super::grid::Grid;
use super::sim::perform;
use crate::build::{BuildJob, Builder, Structure, StatusHandle, Transform};
use crate::interact::{block_distance, Face, BLOCK_REACH};
use crate::movement::Movement;
use crate::pathfind::process::{Act, Arbiter, BlockView, Decision, Look, ProcId, Target};
use crate::pathfind::{NavStatus, SearchJob, Step, answered};
use crate::ActionError;

pub struct World<'a> {
    pub g: &'a Grid,
    pub inv: HashMap<String, u32>,
    pub stations: HashMap<BlockPos, HashMap<String, u32>>,
    /// Acts in order, for checking what came first.
    pub log: Vec<Act>,
    /// Feet where the last run ended.
    pub end: Vec3,
}

impl<'a> World<'a> {
    pub fn new(g: &'a Grid, inv: &[(&str, u32)]) -> Self {
        Self { g, inv: inv.iter().map(|(k, v)| (format!("minecraft:{k}"), *v)).collect(), stations: HashMap::new(), log: Vec::new(), end: [0.0; 3] }
    }
}

struct SimLook<'a>(&'a World<'a>);

impl Look for SimLook<'_> {
    fn solid(&self, p: BlockPos) -> Option<bool> {
        self.block(p).map(|b| b.solid())
    }

    fn find(&self, _: &Target) -> Option<Vec3> {
        None
    }

    fn block(&self, p: BlockPos) -> Option<BlockView> {
        let s = self.0.g.registry.get(self.0.g.view.block(p[0], p[1], p[2]))?;
        Some(BlockView::of(s))
    }

    fn count(&self, item: &str) -> u32 {
        self.0.inv.get(item).copied().unwrap_or(0)
    }

    fn free_slots(&self) -> u32 {
        20
    }
}

pub fn view(g: &Grid, p: BlockPos) -> (String, String) {
    let s = g.registry.get(g.view.block(p[0], p[1], p[2])).expect("known");
    (s.name.to_string(), s.properties.to_string())
}

fn eye(m: &Movement) -> Vec3 {
    let f = m.position().expect("started");
    [f[0], f[1] + 1.62, f[2]]
}

fn in_body(m: &Movement, p: BlockPos) -> bool {
    let f = m.position().expect("started");
    let o = |lo: f32, hi: f32, c: i32| lo < (c + 1) as f32 && hi > c as f32;
    o(f[0] - 0.3, f[0] + 0.3, p[0]) && o(f[1], f[1] + 1.8, p[1]) && o(f[2] - 0.3, f[2] + 0.3, p[2])
}

fn word(face: Face) -> &'static str {
    ["down", "up", "north", "south", "west", "east"][face as usize]
}

/// The state BDS gives `item` placed by this click (place_probe, 2026-10-17); `None`: refused.
fn bds_state(item: &str, face: Face, click: Vec3, eye: Vec3, point: Vec3, cell: &str, pos: BlockPos) -> Option<(String, String)> {
    let n = item.trim_start_matches("minecraft:");
    let top = match face {
        Face::Down => true,
        Face::Up => false,
        _ => click[1] > 0.5,
    };
    let (dx, dz) = (point[0] - eye[0], point[2] - eye[2]);
    let looks = if dx.abs() > dz.abs() { if dx > 0.0 { "east" } else { "west" } } else if dz > 0.0 { "south" } else { "north" };
    let opposite = |d: &str| match d {
        "north" => "south",
        "south" => "north",
        "east" => "west",
        "west" => "east",
        "up" => "down",
        _ => "up",
    };
    let props = match n {
        _ if n.ends_with("_slab") && cell == format!("minecraft:{n}") => return Some((n.replace("_slab", "_double_slab"), "minecraft:vertical_half=bottom".into())),
        _ if n.ends_with("_slab") => format!("minecraft:vertical_half={}", if top { "top" } else { "bottom" }),
        _ if n.ends_with("_log") => format!("pillar_axis={}", match face {
            Face::Up | Face::Down => "y",
            Face::North | Face::South => "z",
            _ => "x",
        }),
        _ if n.ends_with("_stairs") => {
            let weirdo = match looks {
                "east" => 0,
                "west" => 1,
                "south" => 2,
                _ => 3,
            };
            format!("upside_down_bit={},weirdo_direction={weirdo}", u8::from(top))
        }
        "torch" => match face {
            Face::Down => return None,
            Face::Up => "torch_facing_direction=top".into(),
            f => format!("torch_facing_direction={}", opposite(word(f))),
        },
        "furnace" => format!("minecraft:cardinal_direction={}", opposite(looks)),
        "ladder" if matches!(face, Face::Up | Face::Down) => return None,
        "ladder" | "stone_button" => format!("facing_direction={}", face as u8),
        "observer" | "piston" | "dispenser" => {
            // Vertical within a block sideways with the eye over 2 above the block or below it (probe E1-high/low).
            let (above, off) = (eye[1] - pos[1] as f32, (eye[0] - pos[0] as f32 - 0.5).abs().max((eye[2] - pos[2] as f32 - 0.5).abs()));
            let toward = match () {
                _ if off < 1.5 && above > 2.0 => "up",
                _ if off < 1.5 && above < 0.0 => "down",
                _ => opposite(looks),
            };
            let away = |d: &'static str| if n == "dispenser" || (n == "piston" && matches!(d, "up" | "down")) { d } else { opposite(d) };
            let d = away(toward);
            let int = ["down", "up", "north", "south", "west", "east"].iter().position(|w| *w == d).expect("direction");
            if n == "observer" { format!("minecraft:facing_direction={d}") } else { format!("facing_direction={int}") }
        }
        _ => String::new(),
    };
    Some((n.to_string(), props))
}

fn take(w: &mut World, item: &str) -> Result<(), ActionError> {
    let n = w.inv.get_mut(item).filter(|n| **n > 0).ok_or_else(|| ActionError::NotPossible(format!("no {item}")))?;
    *n -= 1;
    Ok(())
}

/// Does an act as the server would, from where the simulated player stands.
fn act(w: &mut World, m: &Movement, act: &Act) -> Result<(), ActionError> {
    w.log.push(act.clone());
    let e = eye(m);
    let far = |p: BlockPos| block_distance(e, p) > BLOCK_REACH;
    let look = SimLook(&*w);
    match act {
        Act::Place { item, against, face, click } => {
            let pos = face.adjacent(*against);
            let cell = look.block(pos).expect("loaded");
            if far(*against) || look.solid(*against) != Some(true) || in_body(m, pos) || (cell.solid() && !cell.name.ends_with("_slab")) {
                return Err(ActionError::Rejected(format!("place {item} at {pos:?} from {e:?}")));
            }
            let point = [against[0] as f32 + click[0], against[1] as f32 + click[1], against[2] as f32 + click[2]];
            let (name, props) = bds_state(item, *face, *click, e, point, cell.name, pos).ok_or_else(|| ActionError::Rejected("refused".into()))?;
            take(w, item)?;
            w.g.set_state(pos, &name, &props);
            Ok(())
        }
        Act::Break { pos } => {
            if far(*pos) {
                return Err(ActionError::NotPossible(format!("break {pos:?} from {e:?}")));
            }
            w.g.set_state(*pos, "air", "");
            Ok(())
        }
        Act::Fill { pos, blocks } => {
            let reach = Face::ALL.iter().any(|f| {
                let o = f.offset();
                let a = [pos[0] - o[0], pos[1] - o[1], pos[2] - o[2]];
                look.solid(a) == Some(true) && !far(a)
            });
            let item = blocks.iter().find(|b| look.count(b) > 0).cloned();
            match item {
                Some(item) if reach && !in_body(m, *pos) => {
                    take(w, &item)?;
                    w.g.set_state(*pos, item.trim_start_matches("minecraft:"), "");
                    Ok(())
                }
                _ => Err(ActionError::NotPossible(format!("fill {pos:?} from {e:?}"))),
            }
        }
        Act::Restock { station, items } => {
            if far(*station) {
                return Err(ActionError::NotPossible("station out of reach".into()));
            }
            let stock = w.stations.get_mut(station).ok_or_else(|| ActionError::NotPossible("no station".into()))?;
            for (item, want) in items {
                let have = stock.get(item).copied().unwrap_or(0);
                let moved = have.min(want.div_ceil(64) * 64);
                stock.insert(item.clone(), have - moved);
                *w.inv.entry(item.clone()).or_default() += moved;
            }
            Ok(())
        }
    }
}

/// A builder for `s` with its minimum corner at `origin`, run alone.
pub fn builder(s: &Structure, origin: BlockPos, stations: &[BlockPos]) -> (Builder, StatusHandle) {
    let mut job = BuildJob::new("sim.mcstructure", Transform { origin, ..Transform::default() });
    (job.stations, job.scaffold) = (stations.to_vec(), vec!["minecraft:dirt".into()]);
    let b = Builder::with_structure(job, s);
    let status = b.status();
    (b, status)
}

/// Runs `arb` from `start` until `id` finishes or `max_ticks` pass; returns the ticks taken.
pub fn run(w: &mut World, start: Vec3, arb: &mut Arbiter, id: ProcId, max_ticks: u32) -> u32 {
    let world = w.g.physics();
    let mut m = Movement::new();
    m.start(start, 0.0, 0.0);
    for tick in 0..max_ticks {
        let pos = m.position().expect("started");
        let decision = arb.decide(pos, m.on_ground(), &SimLook(&*w));
        if tick % 500 == 0 {
            eprintln!("tick {tick} at {pos:?} ground {} {decision:?}", m.on_ground());
        }
        match decision {
            Decision::Go => {
                let pilot = arb.pilot_mut().expect("routed");
                let opts = pilot.search_opts(Some(blocks(64)), 20.0);
                let result = pilot.drive(&w.g.terrain(), &mut m, opts, &mut |job: SearchJob| answered(job.run(&w.g.terrain())));
                let mut placed = Vec::new();
                if result == Ok(NavStatus::Working) {
                    let node = pilot.pending_work().expect("work due");
                    placed = node.work.steps().filter_map(|s| if let Step::Place { against, face } = s { Some(face.adjacent(against)) } else { None }).collect();
                    perform(w.g, &world, &mut m, &node);
                    pilot.work_done();
                }
                if !matches!(result, Ok(NavStatus::Searching | NavStatus::Moving | NavStatus::Working)) {
                    m.controls.stop();
                }
                if let Err(e) = &result {
                    eprintln!("DRIVE FAIL tick {tick} at {pos:?}: {e}");
                }
                arb.placed(placed);
                arb.drove(result, Vec::new());
            }
            Decision::Act(pid, a) => {
                m.controls.stop();
                let result = act(w, &m, &a);
                if let Err(e) = &result {
                    eprintln!("ACT FAIL tick {tick} {a:?}: {e}");
                }
                arb.acted(pid, &a, &result);
            }
            Decision::Hold | Decision::Idle | Decision::Empty => m.controls.stop(),
        }
        w.end = pos;
        if arb.outcome(id).is_some() {
            return tick;
        }
        m.tick(&world);
    }
    max_ticks
}

/// Why each open placement of `s` is not being done from where the last run ended.
pub fn explain(w: &World, s: &Structure, origin: BlockPos) -> Vec<String> {
    builder(s, origin, &[]).0.explain(&SimLook(w), w.end)
}
